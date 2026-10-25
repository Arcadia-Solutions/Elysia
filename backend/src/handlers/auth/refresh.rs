use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::Result;
use crate::services::auth::{self as auth_service, SessionTokens};

/// The refresh token from the last login or refresh.
#[derive(Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[utoipa::path(
    tag = "Auth",
    post,
    path = "/api/auth/refresh",
    request_body = RefreshRequest,
    description = "Trade a refresh token for a new access token and a new refresh token. The old refresh token stops working and the session expiry is pushed back by its lifetime (60 days when remembered, 2 hours otherwise).",
    responses(
        (status = 200, description = "New session tokens", body = SessionTokens),
        (status = 401, description = "Unknown, already used or expired refresh token"),
    )
)]
pub async fn refresh(
    pool: Data<PgPool>,
    config: Data<Config>,
    body: Json<RefreshRequest>,
) -> Result<HttpResponse> {
    let tokens =
        auth_service::refresh_session(pool.get_ref(), &body.refresh_token, &config.auth.jwt_secret)
            .await?;
    Ok(HttpResponse::Ok().json(tokens))
}
