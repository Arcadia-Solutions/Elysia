use actix_web::{
    HttpResponse,
    web::{Data, Json, Query},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use super::upload::{UploadOptionsQuery, UploadResponse};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::middlewares::UploadUser;
use crate::services::files;
use crate::settings::SharedElysiaSettings;
use crate::storage::Storage;

/// A link to an image to rehost.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UploadUrlRequest {
    pub url: String,
}

#[utoipa::path(
    tag = "Images",
    post,
    path = "/api/upload-url",
    request_body = UploadUrlRequest,
    params(UploadOptionsQuery),
    security(("bearer" = []), ("sso_client_basic" = [])),
    responses(
        (status = 200, description = "Image rehosted", body = UploadResponse),
        (status = 400, description = "Could not fetch the url, url not http(s) or not pointing to a public address, file too large, or storage quota exceeded"),
        (status = 401, description = "Missing or invalid session, or invalid SSO client credentials. The SSO provider may upload on behalf of its users with its client credentials as Basic auth and the user subject in the On-Behalf-Of-Sub header (On-Behalf-Of-Username names the account on first upload)"),
        (status = 403, description = "Uploads are disabled"),
        (status = 415, description = "Link does not point to a supported image"),
    )
)]
pub async fn upload_url(
    authenticated: UploadUser,
    config: Data<Config>,
    settings: SharedElysiaSettings,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    query: Query<UploadOptionsQuery>,
    body: Json<UploadUrlRequest>,
) -> Result<HttpResponse> {
    let settings = settings.read().expect("settings lock poisoned").clone();
    let mut options = query.into_inner().into_options(&settings)?;
    options.remaining_storage_bytes = files::remaining_storage_bytes(
        pool.get_ref(),
        authenticated.user_id,
        settings.default_storage_quota_bytes,
    )
    .await?;
    // The guarded client vets redirects and resolved hostnames, not this first URL.
    let url =
        url::Url::parse(&body.url).map_err(|e| Error::BadRequest(format!("invalid url: {e}")))?;
    files::check_public_url(&url).map_err(|e| Error::BadRequest(e.to_string()))?;
    let stored = files::store_from_url(
        pool.get_ref(),
        storage.get_ref(),
        &settings,
        files::http_client(),
        &body.url,
        None,
        &options,
        authenticated.user_id,
        authenticated.api_key_id,
    )
    .await?;

    Ok(HttpResponse::Ok().json(UploadResponse {
        url: stored.image_url(&config.server.public_url),
        thumbnail_url: stored.thumbnail_url(&config.server.public_url),
        id: stored.id,
        ext: stored.ext,
        existed: stored.existed,
        actions: stored.actions,
    }))
}
