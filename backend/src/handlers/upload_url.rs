use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::Result;
use crate::handlers::upload::UploadResponse;
use crate::middlewares::AdminAuth;
use crate::services::files;
use crate::storage::Storage;

/// A link to an image to rehost.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UploadUrlRequest {
    pub url: String,
}

#[utoipa::path(
    post,
    path = "/api/upload-url",
    request_body = UploadUrlRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Image rehosted", body = UploadResponse),
        (status = 400, description = "Could not fetch the url, or file too large"),
        (status = 401, description = "Missing or invalid admin token"),
        (status = 415, description = "Link does not point to a supported image"),
    )
)]
pub async fn upload_url(
    _auth: AdminAuth,
    cfg: Data<Config>,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    body: Json<UploadUrlRequest>,
) -> Result<HttpResponse> {
    let stored =
        files::store_from_url(pool.get_ref(), storage.get_ref(), cfg.get_ref(), &body.url).await?;

    Ok(HttpResponse::Ok().json(UploadResponse {
        url: format!("/i/{}.{}", stored.id, stored.ext),
        id: stored.id,
        ext: stored.ext,
    }))
}
