use actix_web::web::{ServiceConfig, get};

pub mod overview;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.route("/overview", get().to(overview::stats_overview));
}
