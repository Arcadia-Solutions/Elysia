use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::users;
use crate::services::auth as auth_service;

/// Current plus replacement password for a self-service password change.
#[derive(Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[utoipa::path(
    tag = "elysia",
    put,
    path = "/api/auth/password",
    request_body = ChangePasswordRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Password changed"),
        (status = 400, description = "Wrong current password, weak replacement, or SSO account"),
        (status = 401, description = "Missing, invalid or banned session"),
    )
)]
pub async fn change_password(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    body: Json<ChangePasswordRequest>,
) -> Result<HttpResponse> {
    let request = body.into_inner();
    auth_service::validate_password(&request.new_password)?;
    let row = users::find_by_id(pool.get_ref(), authenticated.user_id)
        .await?
        .ok_or(Error::Unauthorized)?;
    if row.banned {
        return Err(Error::Unauthorized);
    }
    let Some(hash) = row.password_hash else {
        return Err(Error::BadRequest(
            "sso accounts have no local password to change".into(),
        ));
    };
    let current_ok = auth_service::verify_password_async(hash, request.current_password).await;
    if !current_ok {
        return Err(Error::BadRequest("current password is wrong".into()));
    }
    let new_hash = auth_service::hash_password_async(request.new_password).await?;
    users::update_password_hash(pool.get_ref(), row.id, &new_hash).await?;
    Ok(HttpResponse::Ok().finish())
}
