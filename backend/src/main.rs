use std::sync::RwLock;

use actix_cors::Cors;
use actix_web::{
    App, HttpServer,
    web::{self, Data},
};
use sqlx::postgres::PgPoolOptions;
use tracing_actix_web::TracingLogger;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use elysia::api_doc::ApiDoc;
use elysia::config::Config;
use elysia::handlers::{
    change_password, get_elysia_settings, get_elysia_unauth_settings, get_public_elysia_settings,
    images, login, me, metadata, put_elysia_settings, register, serve, serve_thumbnail,
    sso_callback, sso_login, stats_overview, upload, upload_url,
};
use elysia::repository::settings as settings_repo;
use elysia::services::sso::SsoState;
use elysia::storage::Storage;
use elysia::telemetry;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfg = Config::load();
    cfg.validate();
    telemetry::init(&cfg.telemetry);
    let (host, port) = (cfg.server.host.clone(), cfg.server.port);

    let pool = PgPoolOptions::new()
        .max_connections(5)
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

    let cfg = Data::new(cfg);
    let pool = Data::new(pool);
    let storage = Data::new(storage);
    let elysia_settings = Data::new(RwLock::new(elysia_settings));
    let sso_state = Data::new(SsoState::default());

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
            .route("/api/auth/register", web::post().to(register))
            .route("/api/auth/login", web::post().to(login))
            .route("/api/auth/me", web::get().to(me))
            .route("/api/auth/password", web::put().to(change_password))
            .route("/api/auth/sso/login", web::get().to(sso_login))
            .route("/api/auth/sso/callback", web::post().to(sso_callback))
            .route("/api/upload", web::post().to(upload))
            .route("/api/upload-url", web::post().to(upload_url))
            .route(
                "/api/public-elysia-settings",
                web::get().to(get_public_elysia_settings),
            )
            .route(
                "/api/elysia-unauth-settings",
                web::get().to(get_elysia_unauth_settings),
            )
            .route("/api/elysia-settings", web::get().to(get_elysia_settings))
            .route("/api/elysia-settings", web::put().to(put_elysia_settings))
            .route("/api/images", web::get().to(images))
            .route("/api/stats/overview", web::get().to(stats_overview))
            .route("/api/i/{id}", web::get().to(metadata))
            .route("/i/{filename}", web::get().to(serve))
            .route("/t/{filename}", web::get().to(serve_thumbnail))
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}")
                    .url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind((host, port))?
    .run()
    .await;

    telemetry::shutdown();
    result
}
