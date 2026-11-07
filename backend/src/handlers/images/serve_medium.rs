use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};

use super::{check_referrer, file_response, parse_filename, trashed_response};
use crate::error::Result;
use crate::services::views::ViewCounter;
use crate::settings::SharedElysiaSettings;
use crate::storage::Storage;

#[utoipa::path(
    tag = "Images",
    get,
    path = "/m/{filename}",
    description = "Served straight from disk, without a database lookup. Falls back to \
        the original file when no medium was generated (the image was within the \
        medium box, or medium images were off at store time): its medium URL then \
        carries the original extension. A trashed image's medium is still served, \
        uncached, when the `Referer` is one of Elysia's own pages.",
    params(("filename" = String, Path, description = "<id>.<ext>")),
    responses(
        (status = 200, description = "The medium bytes, or the original image when it is smaller than the medium box"),
        (status = 403, description = "Referer not in the allowed referrers"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn serve_medium(
    req: HttpRequest,
    storage: Data<Storage>,
    views: Data<ViewCounter>,
    settings: SharedElysiaSettings,
    filename: Path<String>,
) -> Result<HttpResponse> {
    let decision = check_referrer(&req, &settings)?;
    let (id, ext) = parse_filename(&filename)?;
    let file = match storage.open_medium(id) {
        Ok(file) => file,
        Err(_) => match storage.open(id, ext) {
            Ok(file) => file,
            Err(_) => {
                let trashed_file = storage.open_trashed_medium(id, ext);
                return trashed_response(&req, &decision, trashed_file);
            }
        },
    };
    if !decision.from_own_page {
        views.count(id, &decision.referrer, true);
    }
    Ok(file_response(&req, file, decision.cache_control))
}
