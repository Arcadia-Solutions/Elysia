use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use super::login::LoginResponse;
use super::sso_callback::log_in;
use super::sso_login::enabled_sso;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::services::sso as sso_service;
use crate::services::sso::SharedSsoState;

/// The registration token from the callback, plus the user's acceptance.
#[derive(Deserialize, ToSchema)]
pub struct SsoRegisterRequest {
    pub registration_token: String,
    pub accept_legal: bool,
    /// Keep the session 60 days without a refresh instead of 2 hours.
    #[serde(default)]
    pub remember_me: bool,
}

#[utoipa::path(
    tag = "Auth",
    post,
    path = "/api/auth/sso/register",
    request_body = SsoRegisterRequest,
    description = "Create a new single sign-on user once they accepted the terms of service and privacy policy, recording when, and sign them in.",
    responses(
        (status = 200, description = "Session tokens plus caller profile", body = LoginResponse),
        (status = 400, description = "The terms of service and privacy policy were not accepted"),
        (status = 401, description = "Invalid or expired registration token, or banned"),
    )
)]
pub async fn sso_register(
    pool: Data<PgPool>,
    config: Data<Config>,
    sso_state: SharedSsoState,
    request: Json<SsoRegisterRequest>,
) -> Result<HttpResponse> {
    let sso = enabled_sso(&config)?;
    if !request.accept_legal {
        return Err(Error::BadRequest(
            "the terms of service and privacy policy must be accepted".into(),
        ));
    }
    let identity = sso_service::decode_registration_token(
        &request.registration_token,
        &config.auth.jwt_secret,
    )?;
    let login = log_in(
        &pool,
        &config,
        &sso_state,
        sso,
        &identity,
        true,
        request.remember_me,
    )
    .await?;
    Ok(HttpResponse::Ok().json(login))
}
