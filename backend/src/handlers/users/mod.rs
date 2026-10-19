use actix_web::web::{ServiceConfig, get, put};

pub mod get_user;
pub mod list_users;
pub mod set_user_banned;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.route("", get().to(list_users::list_users))
        .route("/{id}", get().to(get_user::get_user))
        .route("/{id}/banned", put().to(set_user_banned::set_user_banned));
}
