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
    tag = "elysia",
    get,
    path = "/i/{filename}",
    description = "Served straight from disk, without a database lookup. A trashed image \
        is still served, uncached, when the `Referer` is one of \
        Elysia's own pages.",
    params(("filename" = String, Path, description = "<id>.<ext>")),
    responses(
        (status = 200, description = "The image bytes"),
        (status = 403, description = "Referer not in the allowed referrers"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn serve(
    req: HttpRequest,
    storage: Data<Storage>,
    views: Data<ViewCounter>,
    settings: SharedElysiaSettings,
    filename: Path<String>,
) -> Result<HttpResponse> {
    let decision = check_referrer(&req, &settings)?;
    let (id, ext) = parse_filename(&filename)?;
    let file = match storage.open(id, ext).await {
        Ok(file) => file,
        Err(_) => {
            let trashed_file = storage.open_trashed(id, ext).await;
            return trashed_response(&req, &decision, trashed_file);
        }
    };
    if !decision.from_own_page {
        views.count(id, false);
    }
    Ok(file_response(&req, file, decision.cache_control))
}
