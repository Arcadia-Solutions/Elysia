use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};

use super::{check_referrer, file_response, parse_filename, trashed_response};
use crate::error::Result;
use crate::services::views::ViewCounter;
use crate::settings::SharedElysiaSettings;
use crate::storage::{Preview, Storage};

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
    serve_preview(
        &req,
        &storage,
        Some(&views),
        &settings,
        &filename,
        Preview::Medium,
    )
}

#[utoipa::path(
    tag = "Images",
    get,
    path = "/t/{filename}",
    description = "The image cropped to the thumbnail box, used by the browse pages. Served \
        like `/m/`, falling back to the original file when no thumbnail was generated. \
        Views are not counted.",
    params(("filename" = String, Path, description = "<id>.<ext>")),
    responses(
        (status = 200, description = "The thumbnail bytes, or the original image when it is within the thumbnail box"),
        (status = 403, description = "Referer not in the allowed referrers"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn serve_thumbnail(
    req: HttpRequest,
    storage: Data<Storage>,
    settings: SharedElysiaSettings,
    filename: Path<String>,
) -> Result<HttpResponse> {
    serve_preview(
        &req,
        &storage,
        None,
        &settings,
        &filename,
        Preview::Thumbnail,
    )
}

/// Serve a preview, or the original file when none was generated, counting a
/// view when `views` is set.
fn serve_preview(
    req: &HttpRequest,
    storage: &Storage,
    views: Option<&ViewCounter>,
    settings: &SharedElysiaSettings,
    filename: &str,
    preview: Preview,
) -> Result<HttpResponse> {
    let decision = check_referrer(req, settings)?;
    let (id, ext) = parse_filename(filename)?;
    let file = match storage.open_preview(id, preview) {
        Ok(file) => file,
        Err(_) => match storage.open(id, ext) {
            Ok(file) => file,
            Err(_) => {
                let trashed_file = storage.open_trashed_preview(id, ext, preview);
                return trashed_response(req, &decision, trashed_file);
            }
        },
    };
    if let Some(views) = views
        && !decision.from_own_page
    {
        views.count(id, &decision.referrer, true);
    }
    Ok(file_response(req, file, decision.cache_control))
}
