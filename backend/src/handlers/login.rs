use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::{Error, Result};

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub token: String,
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    pub valid: bool,
}

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Token is valid", body = LoginResponse),
        (status = 401, description = "Token is invalid"),
    )
)]
pub async fn login(cfg: Data<Config>, body: Json<LoginRequest>) -> Result<HttpResponse> {
    if body.token == cfg.auth.admin_token {
        Ok(HttpResponse::Ok().json(LoginResponse { valid: true }))
    } else {
        Err(Error::Unauthorized)
    }
}
