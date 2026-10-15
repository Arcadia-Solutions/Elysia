use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::handlers::{check_referrer, file_response, id_from_filename};
use crate::services::files;
use crate::settings::SharedElysiaSettings;
use crate::storage::Storage;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/i/{filename}",
    params(("filename" = String, Path, description = "<id>.<ext>")),
    responses(
        (status = 200, description = "The image bytes"),
        (status = 403, description = "Referer not in the allowed referrers"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn serve(
    req: HttpRequest,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    settings: SharedElysiaSettings,
    filename: Path<String>,
) -> Result<HttpResponse> {
    let cache_control = check_referrer(&req, &settings)?;
    let filename = filename.into_inner();
    let row = files::get(pool.get_ref(), id_from_filename(&filename)).await?;

    let file = storage
        .open(&row.id, &row.ext)
        .await
        .map_err(|_| Error::NotFound)?;
    Ok(file_response(&req, file, &row.mime, cache_control))
}
