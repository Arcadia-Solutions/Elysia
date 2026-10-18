use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};

use crate::error::{Error, Result};
use crate::handlers::{check_referrer, file_response, parse_filename};
use crate::services::views::ViewCounter;
use crate::settings::SharedElysiaSettings;
use crate::storage::Storage;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/i/{filename}",
    description = "Served straight from disk, without a database lookup.",
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
    let file = storage.open(id, ext).await.map_err(|_| Error::NotFound)?;
    if decision.count_view {
        views.count(id, false);
    }
    Ok(file_response(&req, file, decision.cache_control))
}
