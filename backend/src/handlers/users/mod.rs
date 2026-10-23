use actix_web::web::{ServiceConfig, get, put};

pub mod get_user;
pub mod list_users;
pub mod set_user_banned;
pub mod set_user_storage_quota;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.route("", get().to(list_users::list_users))
        .route("/{id}", get().to(get_user::get_user))
        .route("/{id}/banned", put().to(set_user_banned::set_user_banned))
        .route(
            "/{id}/storage-quota",
            put().to(set_user_storage_quota::set_user_storage_quota),
        );
}
