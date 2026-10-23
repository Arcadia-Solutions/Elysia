use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::api_keys::{self, ApiKeyScope, ApiKeySummary};
use crate::services::auth as auth_service;

/// Longest accepted API key name.
const MAXIMUM_NAME_LENGTH: usize = 64;

#[derive(Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    /// Label telling the key apart, 1 to 64 characters.
    pub name: String,
    pub scope: ApiKeyScope,
    /// When the key expires, an ISO-8601 timestamp in the future; null never expires.
    #[schema(format = DateTime)]
    pub expires_at: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreatedApiKey {
    pub api_key: ApiKeySummary,
    /// The secret token, shown only this once. Send it as `Authorization: Bearer <token>`.
    pub token: String,
}

#[utoipa::path(
    tag = "API Keys",
    post,
    path = "/api/api-keys",
    description = "Create an API key for the caller. The token is returned only in this response.",
    request_body = CreateApiKeyRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "API key created", body = CreatedApiKey),
        (status = 400, description = "Invalid name or expiry"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Called with an API key"),
        (status = 409, description = "The caller already has an active API key with this name"),
    )
)]
pub async fn create_api_key(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    body: Json<CreateApiKeyRequest>,
) -> Result<HttpResponse> {
    authenticated.require_session()?;
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > MAXIMUM_NAME_LENGTH {
        return Err(Error::BadRequest(
            "name must be between 1 and 64 characters".into(),
        ));
    }
    if let Some(expires_at) = &body.expires_at
        && !api_keys::is_future(pool.get_ref(), expires_at).await?
    {
        return Err(Error::BadRequest("expires_at must be in the future".into()));
    }
    let generated = auth_service::generate_api_key();
    let api_key = api_keys::create(
        pool.get_ref(),
        authenticated.user_id,
        name,
        body.scope,
        &generated.token_hash,
        &generated.token_prefix,
        body.expires_at.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(CreatedApiKey {
        api_key,
        token: generated.token,
    }))
}
