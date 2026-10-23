use actix_web::web::{ServiceConfig, get, post};

pub mod get_chevereto_import_progress;
pub mod get_unreferenced_progress;
pub mod list_chevereto_import_failures;
pub mod list_unreferenced_images;
pub mod scan_unreferenced_images;
pub mod start_chevereto_import;
pub mod stop_chevereto_import;
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
    )
    .route(
        "/chevereto-import",
        post().to(start_chevereto_import::start_chevereto_import),
    )
    .route(
        "/chevereto-import/stop",
        post().to(stop_chevereto_import::stop_chevereto_import),
    )
    .route(
        "/chevereto-import/progress",
        get().to(get_chevereto_import_progress::get_chevereto_import_progress),
    )
    .route(
        "/chevereto-import/failures",
        get().to(list_chevereto_import_failures::list_chevereto_import_failures),
    );
}
