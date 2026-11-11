use actix_web::HttpResponse;
use actix_web::web::{ServiceConfig, get, scope};

use crate::handlers::{
    albums, api_keys, auth, fixtures, images, maintenance, reports, settings, stats, users,
};

pub fn init(cfg: &mut ServiceConfig) {
    cfg.service(
        scope("/api")
            .service(scope("/auth").configure(auth::config))
            .service(scope("/albums").configure(albums::config))
            .service(scope("/api-keys").configure(api_keys::config))
            .service(scope("/users").configure(users::config))
            .service(scope("/fixtures").configure(fixtures::config))
            .service(scope("/stats").configure(stats::config))
            .service(scope("/maintenance").configure(maintenance::config))
            .service(scope("/reports").configure(reports::config))
            .configure(images::config)
            .configure(settings::config),
    )
    .configure(images::config_public)
    // unauthenticated, used by the docker health check
    .route("/health", get().to(|| async { HttpResponse::Ok().await }));
}
