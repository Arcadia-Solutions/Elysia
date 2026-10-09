use std::sync::RwLock;

use actix_cors::Cors;
use actix_multipart::form::{MultipartFormConfig, tempfile::TempFileConfig};
use actix_web::{
    App, HttpServer,
    web::{self, Data},
};
use sqlx::postgres::PgPoolOptions;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use elysia::api_doc::ApiDoc;
use elysia::config::Config;
use elysia::handlers::{
    get_elysia_settings, get_public_elysia_settings, login, metadata, put_elysia_settings, serve,
    serve_thumbnail, upload, upload_url,
};
use elysia::repository::settings as settings_repo;
use elysia::storage::Storage;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfg = Config::load();
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

    // Hard multipart stream ceiling, baked in at startup. The per-request size
    // cap (max_file_size_bytes) is enforced in the store path, so it can change
    // in the UI without a restart.
    let upload_limit = cfg.storage.max_upload_stream_bytes as usize;
    let upload_dir = cfg.storage.upload_dir.clone();

    let cfg = Data::new(cfg);
    let pool = Data::new(pool);
    let storage = Data::new(storage);
    let elysia_settings = Data::new(RwLock::new(elysia_settings));

    println!("Server running at http://{host}:{port}");

    HttpServer::new(move || {
        App::new()
            .wrap(Cors::permissive())
            .app_data(cfg.clone())
            .app_data(pool.clone())
            .app_data(storage.clone())
            .app_data(elysia_settings.clone())
            .app_data(MultipartFormConfig::default().total_limit(upload_limit))
            .app_data(TempFileConfig::default().directory(&upload_dir))
            .route("/api/auth/login", web::post().to(login))
            .route("/api/upload", web::post().to(upload))
            .route("/api/upload-url", web::post().to(upload_url))
            .route(
                "/api/public-elysia-settings",
                web::get().to(get_public_elysia_settings),
            )
            .route("/api/elysia-settings", web::get().to(get_elysia_settings))
            .route("/api/elysia-settings", web::put().to(put_elysia_settings))
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
    .await
}
