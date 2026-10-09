use actix_web::{
    HttpResponse,
    web::{Data, Path},
};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::Result;
use crate::services::files;

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
    /// Upload time as a UTC ISO-8601 string.
    pub created_at: String,
    /// Public URL for the raw image.
    pub url: String,
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
pub async fn metadata(pool: Data<PgPool>, id: Path<String>) -> Result<HttpResponse> {
    let row = files::get_metadata(pool.get_ref(), &id.into_inner()).await?;
    Ok(HttpResponse::Ok().json(MetadataResponse {
        url: format!("/i/{}.{}", row.id, row.ext),
        id: row.id,
        ext: row.ext,
        mime: row.mime,
        original_name: row.original_name,
        size: row.size,
        width: row.width,
        height: row.height,
        created_at: row.created_at,
    }))
}
