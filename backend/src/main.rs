use std::sync::RwLock;

use actix_cors::Cors;
use actix_web::{App, HttpServer, web::Data};
use sqlx::postgres::PgPoolOptions;
use tracing_actix_web::TracingLogger;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use elysia::api_doc::ApiDoc;
use elysia::config::Config;
use elysia::middlewares::RateLimits;
use elysia::repository::settings as settings_repo;
use elysia::routes;
use elysia::scheduler;
use elysia::services::sso::SsoState;
use elysia::services::views::ViewCounter;
use elysia::storage::Storage;
use elysia::telemetry;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfg = Config::load();
    cfg.validate();
    if let Some(proxy) = &cfg.outbound_proxy {
        elysia::config::set_outbound_proxy(proxy);
    }
    telemetry::init(&cfg.telemetry);
    let (host, port) = (cfg.server.host.clone(), cfg.server.port);

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url())
        .await
        .expect("failed to connect to database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let storage = Storage::new(&cfg.storage.upload_dir);
    storage
        .ensure_root()
        .await
        .expect("failed to create upload dir");

    // Runtime elysia settings live in the DB and are edited from the web UI.
    let elysia_settings = settings_repo::load(&pool)
        .await
        .expect("failed to load elysia settings");

    let rate_limits = Data::new(RateLimits::new(cfg.rate_limit.as_ref()));
    let cfg = Data::new(cfg);
    let pool = Data::new(pool);
    let storage = Data::new(storage);
    let elysia_settings = Data::new(RwLock::new(elysia_settings));
    let sso_state = Data::new(SsoState::default());
    let views = Data::new(ViewCounter::default());

    let scheduler_handle = tokio::spawn({
        let (pool, storage, views, rate_limits) = (
            pool.clone(),
            storage.clone(),
            views.clone(),
            rate_limits.clone(),
        );
        async move { scheduler::handle(&pool, &storage, &views, &rate_limits).await }
    });
    let (shutdown_pool, shutdown_views) = (pool.clone(), views.clone());

    tracing::info!("Server running at http://{host}:{port}");

    let result = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .wrap(Cors::permissive())
            .app_data(cfg.clone())
            .app_data(pool.clone())
            .app_data(storage.clone())
            .app_data(elysia_settings.clone())
            .app_data(sso_state.clone())
            .app_data(views.clone())
            .app_data(rate_limits.clone())
            .configure(routes::init)
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}")
                    .url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind((host, port))?
    .run()
    .await;

    // Stop the scheduler before the last flush, so the two never race.
    scheduler_handle.abort();
    scheduler::flush(&shutdown_pool, &shutdown_views).await;
    telemetry::shutdown();
    result
}
