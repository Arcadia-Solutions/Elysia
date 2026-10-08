use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use actix_web::{HttpResponse, web::Data};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::Result;
use crate::middlewares::AdminAuth;
use crate::services::files;
use crate::storage::Storage;

#[derive(Debug, MultipartForm, ToSchema)]
pub struct UploadForm {
    // No per-field limit: the overall cap is MultipartFormConfig::total_limit,
    // set from config.storage.max_file_size_bytes in main.rs. TempFile streams to disk,
    // so large uploads never buffer in RAM.
    #[schema(value_type = String, format = Binary, content_media_type = "application/octet-stream")]
    pub file: TempFile,
}

#[derive(Serialize, ToSchema)]
pub struct UploadResponse {
    pub id: String,
    pub ext: String,
    pub url: String,
    /// True when the image was already hosted and this is the existing one
    /// (deduplicated), false when it was newly stored.
    pub existed: bool,
}

#[utoipa::path(
    post,
    path = "/api/upload",
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Image uploaded", body = UploadResponse),
        (status = 400, description = "File too large"),
        (status = 401, description = "Missing or invalid admin token"),
        (status = 415, description = "Not a supported image"),
    )
)]
pub async fn upload(
    _auth: AdminAuth,
    cfg: Data<Config>,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    MultipartForm(form): MultipartForm<UploadForm>,
) -> Result<HttpResponse> {
    let stored =
        files::store_upload(pool.get_ref(), storage.get_ref(), cfg.get_ref(), form.file).await?;

    Ok(HttpResponse::Ok().json(UploadResponse {
        url: format!("/i/{}.{}", stored.id, stored.ext),
        id: stored.id,
        ext: stored.ext,
        existed: stored.existed,
    }))
}
