use actix_web::web::{ServiceConfig, get, post};

pub mod get_unreferenced_progress;
pub mod list_unreferenced_images;
pub mod scan_unreferenced_images;
pub mod trash_unreferenced_images;

pub fn config(cfg: &mut ServiceConfig) {
    cfg.route(
        "/unreferenced-images",
        get().to(list_unreferenced_images::list_unreferenced_images),
    )
    .route(
        "/unreferenced-images/scan",
        post().to(scan_unreferenced_images::scan_unreferenced_images),
    )
    .route(
        "/unreferenced-images/trash",
        post().to(trash_unreferenced_images::trash_unreferenced_images),
    )
    .route(
        "/unreferenced-images/progress",
        get().to(get_unreferenced_progress::get_unreferenced_progress),
    );
}
