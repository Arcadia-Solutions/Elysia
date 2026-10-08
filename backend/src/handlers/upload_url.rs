use actix_web::{
    HttpResponse,
    web::{Data, Json, Query},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::Result;
use crate::handlers::settings::SharedElysiaSettings;
use crate::handlers::upload::{UploadOptionsQuery, UploadResponse};
use crate::middlewares::AdminAuth;
use crate::services::files;
use crate::storage::Storage;

/// A link to an image to rehost.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UploadUrlRequest {
    pub url: String,
}

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/upload-url",
    request_body = UploadUrlRequest,
    params(UploadOptionsQuery),
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
    settings: SharedElysiaSettings,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    query: Query<UploadOptionsQuery>,
    body: Json<UploadUrlRequest>,
) -> Result<HttpResponse> {
    let settings = *settings.read().expect("settings lock poisoned");
    let options = query.into_inner().into_options(&settings)?;
    let stored = files::store_from_url(
        pool.get_ref(),
        storage.get_ref(),
        &settings,
        &body.url,
        &options,
    )
    .await?;

    Ok(HttpResponse::Ok().json(UploadResponse {
        url: format!("/i/{}.{}", stored.id, stored.ext),
        id: stored.id,
        ext: stored.ext,
        existed: stored.existed,
        actions: stored.actions,
    }))
}
