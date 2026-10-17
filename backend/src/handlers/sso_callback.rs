use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::handlers::login::LoginResponse;
use crate::handlers::me::ProfileResponse;
use crate::handlers::sso_login::enabled_sso;
use crate::repository::users;
use crate::services::sso::SharedSsoState;
use crate::services::{auth as auth_service, sso as sso_service};

/// The provider's code, plus the verifier the frontend kept since the login
/// started. The frontend already checked the state.
#[derive(Deserialize, ToSchema)]
pub struct SsoCallbackRequest {
    pub code: String,
    pub code_verifier: String,
}

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/auth/sso/callback",
    request_body = SsoCallbackRequest,
    responses(
        (status = 200, description = "Signed session token plus caller profile", body = LoginResponse),
        (status = 400, description = "Malformed provider response"),
        (status = 401, description = "Failed provider verification or banned"),
    )
)]
pub async fn sso_callback(
    pool: Data<PgPool>,
    config: Data<Config>,
    sso_state: SharedSsoState,
    request: Json<SsoCallbackRequest>,
) -> Result<HttpResponse> {
    let sso = enabled_sso(&config)?;
    let identity =
        sso_service::fetch_identity(&sso_state, sso, &request.code, &request.code_verifier).await?;
    let row = users::upsert_sso_user(
        pool.get_ref(),
        sso.normalized_issuer(),
        &identity.sub,
        identity.preferred_username.as_deref().unwrap_or_default(),
        identity.email.as_deref(),
    )
    .await?;
    if row.banned || !sso_service::user_active(&sso_state, sso, row.id, &identity.sub).await {
        return Err(Error::Unauthorized);
    }
    let _ = users::update_last_seen(pool.get_ref(), row.id).await;
    let token = auth_service::mint_jwt(&row.id, &config.auth.jwt_secret)?;
    Ok(HttpResponse::Ok().json(LoginResponse {
        token,
        user: ProfileResponse::from(row),
    }))
}
