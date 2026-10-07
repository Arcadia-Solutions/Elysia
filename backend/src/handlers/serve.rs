use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::services::files;
use crate::storage::Storage;

#[utoipa::path(
    get,
    path = "/i/{filename}",
    params(("filename" = String, Path, description = "<id>.<ext>")),
    responses(
        (status = 200, description = "The image bytes"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn serve(
    req: HttpRequest,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    filename: Path<String>,
) -> Result<HttpResponse> {
    let filename = filename.into_inner();
    let id = filename
        .rsplit_once('.')
        .map_or(filename.as_str(), |(id, _)| id);

    let row = files::get(pool.get_ref(), id).await?;

    // NamedFile streams the bytes (no full read into RAM) and handles range
    // requests + conditional GET (ETag/Last-Modified) for free.
    let file = storage
        .open(&row.id, &row.ext)
        .await
        .map_err(|_| Error::NotFound)?;

    let mut res = file.into_response(&req);
    res.headers_mut().insert(
        actix_web::http::header::CACHE_CONTROL,
        actix_web::http::header::HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    // Trust the DB's sniffed MIME over NamedFile's extension guess.
    if let Ok(mime) = row.mime.parse() {
        res.headers_mut()
            .insert(actix_web::http::header::CONTENT_TYPE, mime);
    }
    Ok(res)
}
