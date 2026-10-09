use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::handlers::{file_response, id_from_filename};
use crate::services::files;
use crate::storage::Storage;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/t/{filename}",
    params(("filename" = String, Path, description = "<id>.<ext>")),
    responses(
        (status = 200, description = "The thumbnail bytes, or the original image when it is smaller than the thumbnail box"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn serve_thumbnail(
    req: HttpRequest,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    filename: Path<String>,
) -> Result<HttpResponse> {
    let filename = filename.into_inner();
    let row = files::get(pool.get_ref(), id_from_filename(&filename)).await?;

    // Serve the thumbnail when one is on disk. Fall back to the main file
    // otherwise: the image was within the box, thumbnails were off at store time,
    // or a thumbnail write failed after the row was inserted.
    if row.has_thumbnail
        && let Ok(file) = storage.open_thumbnail(&row.id).await
    {
        return Ok(file_response(&req, file, "image/webp"));
    }

    let file = storage
        .open(&row.id, &row.ext)
        .await
        .map_err(|_| Error::NotFound)?;
    Ok(file_response(&req, file, &row.mime))
}
