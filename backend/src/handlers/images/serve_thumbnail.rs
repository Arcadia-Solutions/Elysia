use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};

use super::{check_referrer, file_response, parse_filename};
use crate::error::{Error, Result};
use crate::services::views::ViewCounter;
use crate::settings::SharedElysiaSettings;
use crate::storage::Storage;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/t/{filename}",
    description = "Served straight from disk, without a database lookup. Falls back to \
        the original file when no thumbnail was generated (the image was within the \
        thumbnail box, or thumbnails were off at store time): its thumbnail URL then \
        carries the original extension.",
    params(("filename" = String, Path, description = "<id>.<ext>")),
    responses(
        (status = 200, description = "The thumbnail bytes, or the original image when it is smaller than the thumbnail box"),
        (status = 403, description = "Referer not in the allowed referrers"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn serve_thumbnail(
    req: HttpRequest,
    storage: Data<Storage>,
    views: Data<ViewCounter>,
    settings: SharedElysiaSettings,
    filename: Path<String>,
) -> Result<HttpResponse> {
    let decision = check_referrer(&req, &settings)?;
    let (id, ext) = parse_filename(&filename)?;
    let file = match storage.open_thumbnail(id).await {
        Ok(file) => file,
        Err(_) => storage.open(id, ext).await.map_err(|_| Error::NotFound)?,
    };
    if decision.count_view {
        views.count(id, true);
    }
    Ok(file_response(&req, file, decision.cache_control))
}
