use actix_web::{
    HttpResponse,
    web::{Data, Path},
};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::Result;
use crate::services::files::{self, image_url, thumbnail_url};

/// Display metadata for the `/i/<id>` viewer page.
#[derive(Serialize, ToSchema)]
pub struct MetadataResponse {
    pub id: String,
    pub ext: String,
    pub mime: String,
    /// Original filename from the upload, if the client sent one.
    pub original_name: Option<String>,
    pub size: i64,
    pub width: i32,
    pub height: i32,
    /// Source extension before processing; differs from `ext` when converted.
    pub original_ext: String,
    /// Source dimensions before processing; differ from the stored ones when resized.
    pub original_width: i32,
    pub original_height: i32,
    /// Lossy quality applied while processing; null means lossless.
    pub applied_quality: Option<i32>,
    /// Times the image was served from `/i/`.
    pub views: i64,
    /// Times the thumbnail was served from `/t/`.
    pub thumbnail_views: i64,
    pub uploader_username: String,
    /// Upload time as a UTC ISO-8601 string.
    pub created_at: String,
    /// Public URL for the raw image.
    pub url: String,
    /// Public URL for the thumbnail.
    pub thumbnail_url: String,
}

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/i/{id}",
    params(("id" = String, Path, description = "File id")),
    responses(
        (status = 200, description = "File metadata", body = MetadataResponse),
        (status = 404, description = "Not found"),
    )
)]
pub async fn metadata(
    config: Data<Config>,
    pool: Data<PgPool>,
    id: Path<String>,
) -> Result<HttpResponse> {
    let row = files::get_metadata(pool.get_ref(), &id.into_inner()).await?;
    Ok(HttpResponse::Ok().json(MetadataResponse {
        url: image_url(&config.server.public_url, &row.id, &row.ext),
        thumbnail_url: thumbnail_url(
            &config.server.public_url,
            &row.id,
            &row.ext,
            row.has_thumbnail,
        ),
        id: row.id,
        ext: row.ext,
        mime: row.mime,
        original_name: row.original_name,
        size: row.size,
        width: row.width,
        height: row.height,
        original_ext: row.original_ext,
        original_width: row.original_width,
        original_height: row.original_height,
        applied_quality: row.applied_quality,
        views: row.views,
        thumbnail_views: row.thumbnail_views,
        uploader_username: row.uploader_username,
        created_at: row.created_at,
    }))
}
