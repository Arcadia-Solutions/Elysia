use actix_web::web::{ServiceConfig, post, resource};

pub mod create_api_key;
pub mod list_api_keys;
pub mod revoke_api_key;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.service(
        resource("")
            .get(list_api_keys::list_api_keys)
            .post(create_api_key::create_api_key),
    )
    .route("/{id}/revoke", post().to(revoke_api_key::revoke_api_key));
}
