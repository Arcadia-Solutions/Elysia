use actix_web::web::{ServiceConfig, get, post, put};

pub mod change_password;
pub mod login;
pub mod me;
pub mod register;
pub mod sso_callback;
pub mod sso_login;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.route("/register", post().to(register::register))
        .route("/login", post().to(login::login))
        .route("/me", get().to(me::me))
        .route("/password", put().to(change_password::change_password))
        .route("/sso/login", get().to(sso_login::sso_login))
        .route("/sso/callback", post().to(sso_callback::sso_callback));
}
