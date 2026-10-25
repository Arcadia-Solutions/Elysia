use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use super::login::LoginResponse;
use super::me::ProfileResponse;
use super::sso_login::enabled_sso;
use crate::config::{Config, SsoConfig};
use crate::error::{Error, Result};
use crate::repository::users;
use crate::services::sso::{Identity, SharedSsoState};
use crate::services::{auth as auth_service, sso as sso_service};
use crate::settings::SharedElysiaSettings;

/// The provider's code, plus the verifier the frontend kept since the login
/// started. The frontend already checked the state.
#[derive(Deserialize, ToSchema)]
pub struct SsoCallbackRequest {
    pub code: String,
    pub code_verifier: String,
    /// Keep the session 60 days without a refresh instead of 2 hours.
    #[serde(default)]
    pub remember_me: bool,
}

/// Either a session (`token`, `refresh_token` and `user`), or, for a new user who must first
/// accept the terms of service and privacy policy, a `registration_token` to
/// send to `/api/auth/sso/register` along with the acceptance.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct SsoCallbackResponse {
    pub token: Option<String>,
    pub refresh_token: Option<String>,
    pub user: Option<ProfileResponse>,
    pub registration_token: Option<String>,
}

#[utoipa::path(
    tag = "Auth",
    post,
    path = "/api/auth/sso/callback",
    request_body = SsoCallbackRequest,
    description = "Finish a single sign-on. A new user is only created here when no terms of service or privacy policy is set; otherwise nothing is stored until they are accepted through `/api/auth/sso/register`.",
    responses(
        (status = 200, description = "Session, or a registration token for a new user", body = SsoCallbackResponse),
        (status = 400, description = "Malformed provider response"),
        (status = 401, description = "Failed provider verification or banned"),
    )
)]
pub async fn sso_callback(
    pool: Data<PgPool>,
    config: Data<Config>,
    sso_state: SharedSsoState,
    settings: SharedElysiaSettings,
    request: Json<SsoCallbackRequest>,
) -> Result<HttpResponse> {
    let sso = enabled_sso(&config)?;
    let identity =
        sso_service::fetch_identity(&sso_state, sso, &request.code, &request.code_verifier).await?;
    let legal_set = settings.read().expect("settings lock poisoned").legal_set();
    if legal_set
        && users::find_by_issuer_sub(pool.get_ref(), sso.normalized_issuer(), &identity.sub)
            .await?
            .is_none()
    {
        return Ok(HttpResponse::Ok().json(SsoCallbackResponse {
            token: None,
            refresh_token: None,
            user: None,
            registration_token: Some(sso_service::mint_registration_token(
                identity,
                &config.auth.jwt_secret,
            )?),
        }));
    }
    let login = log_in(
        &pool,
        &config,
        &sso_state,
        sso,
        &identity,
        false,
        request.remember_me,
    )
    .await?;
    Ok(HttpResponse::Ok().json(SsoCallbackResponse {
        token: Some(login.token),
        refresh_token: Some(login.refresh_token),
        user: Some(login.user),
        registration_token: None,
    }))
}

/// Find or create the user behind `identity`, refuse them if banned locally or
/// at the provider, and mint their session.
pub(crate) async fn log_in(
    pool: &PgPool,
    config: &Config,
    sso_state: &SharedSsoState,
    sso: &SsoConfig,
    identity: &Identity,
    accepted_legal: bool,
    remember_me: bool,
) -> Result<LoginResponse> {
    let row = users::upsert_sso_user(
        pool,
        sso.normalized_issuer(),
        &identity.sub,
        identity.preferred_username.as_deref().unwrap_or_default(),
        identity.email.as_deref(),
        accepted_legal,
    )
    .await?;
    if row.banned || !sso_service::user_active(sso_state, sso, row.id, &identity.sub).await {
        return Err(Error::Unauthorized);
    }
    let tokens =
        auth_service::start_session(pool, &row.id, remember_me, &config.auth.jwt_secret).await?;
    Ok(LoginResponse {
        token: tokens.token,
        refresh_token: tokens.refresh_token,
        user: ProfileResponse::from(row),
    })
}
