use actix_web::web::{ServiceConfig, post, resource};

pub mod add_album_images;
pub mod create_album;
pub mod delete_album;
pub mod get_album;
pub mod list_albums;
pub mod remove_album_images;
pub mod rename_album;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.service(
        resource("")
            .get(list_albums::list_albums)
            .post(create_album::create_album),
    )
    .service(
        resource("/{id}")
            .get(get_album::get_album)
            .put(rename_album::rename_album)
            .delete(delete_album::delete_album),
    )
    .route(
        "/{id}/images",
        post().to(add_album_images::add_album_images),
    )
    .route(
        "/{id}/images/remove",
        post().to(remove_album_images::remove_album_images),
    );
}
