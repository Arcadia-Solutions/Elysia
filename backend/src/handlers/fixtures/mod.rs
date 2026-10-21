use actix_web::web::{ServiceConfig, get, post};

pub mod generate_fixtures;
pub mod get_fixtures_progress;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.route("", post().to(generate_fixtures::generate_fixtures))
        .route("", get().to(get_fixtures_progress::get_fixtures_progress));
}
