use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::handlers::me::ProfileResponse;
use crate::repository::users;
use crate::services::auth as auth_service;

/// Username-or-email plus password sign-in.
#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

/// A signed session token plus the caller's profile, so login seeds client
/// state without a second request. New `ProfileResponse` fields show up here
/// automatically.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub user: ProfileResponse,
}

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Signed session token plus caller profile", body = LoginResponse),
        (status = 401, description = "Unknown login, wrong password or banned"),
    )
)]
pub async fn login(
    pool: Data<PgPool>,
    config: Data<Config>,
    body: Json<LoginRequest>,
) -> Result<HttpResponse> {
    let request = body.into_inner();
    let row = users::find_by_username_or_email(pool.get_ref(), &request.login).await?;
    // Always run an Argon2 verification so unknown users and SSO rows take
    // as long as a wrong password, avoiding account enumeration by timing.
    // SSO shadow rows have no password and can never log in this way.
    let hash = row
        .as_ref()
        .and_then(|row| row.password_hash.clone())
        .unwrap_or_else(|| auth_service::dummy_hash().to_string());
    let password_ok = auth_service::verify_password_async(hash, request.password).await;
    let Some(row) = row else {
        return Err(Error::Unauthorized);
    };
    if !password_ok || row.password_hash.is_none() || row.banned {
        return Err(Error::Unauthorized);
    }
    let token = auth_service::mint_jwt(&row.id, &config.auth.jwt_secret)?;
    Ok(HttpResponse::Ok().json(LoginResponse {
        token,
        user: ProfileResponse::from(row),
    }))
}
