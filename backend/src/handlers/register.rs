use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::{Error, Result};
use crate::handlers::settings::SharedElysiaSettings;
use crate::repository::users;
use crate::services::auth as auth_service;

/// Registration details for a new local account.
#[derive(Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// The account that was created.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct RegisterResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub is_admin: bool,
}

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Account created", body = RegisterResponse),
        (status = 400, description = "Invalid username, email or password"),
        (status = 403, description = "Open registration is disabled in settings"),
        (status = 409, description = "Username or email already taken"),
    )
)]
pub async fn register(
    pool: Data<PgPool>,
    settings: SharedElysiaSettings,
    body: Json<RegisterRequest>,
) -> Result<HttpResponse> {
    if !settings
        .read()
        .expect("settings lock poisoned")
        .allow_registration
    {
        return Err(Error::Forbidden);
    }
    let request = body.into_inner();
    let username = auth_service::normalize_username(&request.username);
    let email = auth_service::normalize_email(&request.email);
    if !auth_service::valid_username(&username) {
        return Err(Error::BadRequest(
            "username must be 3-32 characters of letters, digits, _ or -".into(),
        ));
    }
    if !auth_service::valid_email(&email) {
        return Err(Error::BadRequest(
            "email must be like name@example.com".into(),
        ));
    }
    auth_service::validate_password(&request.password)?;
    // The database seeds a default admin; registrations never grant admin.
    let password_hash = auth_service::hash_password_async(request.password).await?;
    let row = users::create_local_user(
        pool.get_ref(),
        &username,
        Some(&email),
        &password_hash,
        false,
    )
    .await?;
    Ok(HttpResponse::Created().json(RegisterResponse {
        id: row.id.to_string(),
        username: row.username,
        email: row.email.unwrap_or_default(),
        is_admin: row.is_admin,
    }))
}
