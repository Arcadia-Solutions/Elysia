use actix_web::middleware::from_fn;
use actix_web::web::{ServiceConfig, get, resource};

use crate::middlewares::rate_limit::limit_unauthenticated;

pub mod get_elysia_settings;
pub mod get_elysia_unauth_settings;
pub mod get_public_elysia_settings;
pub mod put_elysia_settings;

/// Routes under `/api`.
pub fn config(cfg: &mut ServiceConfig) {
    cfg.service(
        resource("/elysia-settings")
            .get(get_elysia_settings::get_elysia_settings)
            .put(put_elysia_settings::put_elysia_settings),
    )
    .route(
        "/public-elysia-settings",
        get().to(get_public_elysia_settings::get_public_elysia_settings),
    )
    .route(
        "/elysia-unauth-settings",
        get()
            .to(get_elysia_unauth_settings::get_elysia_unauth_settings)
            .wrap(from_fn(limit_unauthenticated)),
    );
}
