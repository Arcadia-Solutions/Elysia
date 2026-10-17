use actix_web::{
    HttpResponse,
    web::{Data, Json, Query},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::Result;
use crate::handlers::upload::{UploadOptionsQuery, UploadResponse};
use crate::middlewares::AuthUser;
use crate::services::files;
use crate::settings::SharedElysiaSettings;
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
        (status = 401, description = "Missing or invalid session"),
        (status = 415, description = "Link does not point to a supported image"),
    )
)]
pub async fn upload_url(
    authenticated: AuthUser,
    settings: SharedElysiaSettings,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    query: Query<UploadOptionsQuery>,
    body: Json<UploadUrlRequest>,
) -> Result<HttpResponse> {
    let settings = settings.read().expect("settings lock poisoned").clone();
    let options = query.into_inner().into_options(&settings)?;
    let stored = files::store_from_url(
        pool.get_ref(),
        storage.get_ref(),
        &settings,
        files::http_client(),
        &body.url,
        None,
        &options,
        authenticated.user_id,
    )
    .await?;

    Ok(HttpResponse::Ok().json(UploadResponse {
        url: stored.image_url(),
        thumbnail_url: stored.thumbnail_url(),
        id: stored.id,
        ext: stored.ext,
        existed: stored.existed,
        actions: stored.actions,
    }))
}
