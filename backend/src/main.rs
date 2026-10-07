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
use elysia::handlers::{login, serve, upload};
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

    // Upload cap: config.max_file_size_bytes, or ~10 GiB when 0 (= "no cap"). TempFile
    // streams to disk so this isn't held in memory.
    let upload_limit = if cfg.storage.max_file_size_bytes > 0 {
        cfg.storage.max_file_size_bytes as usize
    } else {
        10 * 1024 * 1024 * 1024
    };
    let upload_dir = cfg.storage.upload_dir.clone();

    let cfg = Data::new(cfg);
    let pool = Data::new(pool);
    let storage = Data::new(storage);

    println!("Server running at http://{host}:{port}");

    HttpServer::new(move || {
        App::new()
            .wrap(Cors::permissive())
            .app_data(cfg.clone())
            .app_data(pool.clone())
            .app_data(storage.clone())
            .app_data(MultipartFormConfig::default().total_limit(upload_limit))
            .app_data(TempFileConfig::default().directory(&upload_dir))
            .route("/api/auth/login", web::post().to(login))
            .route("/api/upload", web::post().to(upload))
            .route("/i/{filename}", web::get().to(serve))
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}")
                    .url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind((host, port))?
    .run()
    .await
}
