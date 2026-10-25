use actix_web::web::{ServiceConfig, get, post};

pub mod dismiss_reports;
pub mod list_reports;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.route("", get().to(list_reports::list_reports))
        .route("/dismiss", post().to(dismiss_reports::dismiss_reports));
}
