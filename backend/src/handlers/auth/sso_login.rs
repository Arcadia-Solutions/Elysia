use actix_web::{HttpResponse, web::Data};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::{Config, SsoConfig};
use crate::error::{Error, Result};
use crate::services::sso as sso_service;
use crate::services::sso::SharedSsoState;

/// The provider authorize URL plus what the frontend keeps for the callback:
/// the state, so a callback started by someone else is rejected, and the PKCE
/// verifier, sent back with the code.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct SsoLoginResponse {
    pub authorization_url: String,
    pub state: String,
    pub code_verifier: String,
}

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/auth/sso/login",
    responses(
        (status = 200, description = "Provider authorize URL to redirect the browser to", body = SsoLoginResponse),
        (status = 400, description = "SSO is disabled"),
    )
)]
pub async fn sso_login(config: Data<Config>, sso_state: SharedSsoState) -> Result<HttpResponse> {
    let sso = enabled_sso(&config)?;
    let state = sso_service::generate_state();
    let code_verifier = sso_service::generate_verifier();
    let challenge = sso_service::code_challenge(&code_verifier);
    let authorization_url =
        sso_service::authorization_url(&sso_state, sso, &state, &challenge).await?;
    Ok(HttpResponse::Ok().json(SsoLoginResponse {
        authorization_url,
        state,
        code_verifier,
    }))
}

/// The SSO settings, or a 400 when sign-on is not configured.
pub(crate) fn enabled_sso(config: &Config) -> Result<&SsoConfig> {
    config
        .auth
        .sso
        .as_ref()
        .ok_or_else(|| Error::BadRequest("sso is disabled".into()))
}
