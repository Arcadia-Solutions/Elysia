use actix_web::middleware::from_fn;
use actix_web::web::{ServiceConfig, get, post, put};

use crate::middlewares::rate_limit::{limit_login, limit_unauthenticated};

pub mod change_password;
pub mod login;
pub mod logout;
pub mod me;
pub mod register;
pub mod sso_callback;
pub mod sso_login;
pub mod sso_register;

pub fn config(cfg: &mut ServiceConfig) {
    let unauthenticated = || from_fn(limit_unauthenticated);
    cfg.route(
        "/register",
        post()
            .to(register::register)
            .wrap(from_fn(limit_login))
            .wrap(unauthenticated()),
    )
    .route(
        "/login",
        post()
            .to(login::login)
            .wrap(from_fn(limit_login))
            .wrap(unauthenticated()),
    )
    .route("/logout", post().to(logout::logout))
    .route("/me", get().to(me::me))
    .route(
        "/password",
        put()
            .to(change_password::change_password)
            .wrap(from_fn(limit_login)),
    )
    .route(
        "/sso/login",
        get().to(sso_login::sso_login).wrap(unauthenticated()),
    )
    .route(
        "/sso/callback",
        post()
            .to(sso_callback::sso_callback)
            .wrap(unauthenticated()),
    )
    .route(
        "/sso/register",
        post()
            .to(sso_register::sso_register)
            .wrap(unauthenticated()),
    );
}
