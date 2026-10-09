use std::sync::RwLock;

use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::middlewares::AdminAuth;
use crate::repository::settings as settings_repo;
use crate::settings::{ElysiaSettings, PublicElysiaSettings};

/// Shared, hot-swappable elysia settings. Uploads read a snapshot; a save
/// replaces the whole value, so a change applies without a restart.
pub type SharedElysiaSettings = Data<RwLock<ElysiaSettings>>;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/elysia-settings",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Current elysia settings", body = ElysiaSettings),
        (status = 401, description = "Missing or invalid admin token"),
    )
)]
pub async fn get_elysia_settings(
    _auth: AdminAuth,
    settings: SharedElysiaSettings,
) -> Result<HttpResponse> {
    let current = *settings.read().expect("settings lock poisoned");
    Ok(HttpResponse::Ok().json(current))
}

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/public-elysia-settings",
    description = "The public subset of the settings, so the upload page can validate files client-side before sending them.",
    responses(
        (status = 200, body = PublicElysiaSettings),
    )
)]
pub async fn get_public_elysia_settings(settings: SharedElysiaSettings) -> Result<HttpResponse> {
    let current = *settings.read().expect("settings lock poisoned");
    Ok(HttpResponse::Ok().json(PublicElysiaSettings::from(current)))
}

#[utoipa::path(
    tag = "elysia",
    put,
    path = "/api/elysia-settings",
    request_body = ElysiaSettings,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Settings saved", body = ElysiaSettings),
        (status = 400, description = "Contradictory settings"),
        (status = 401, description = "Missing or invalid admin token"),
    )
)]
pub async fn put_elysia_settings(
    _auth: AdminAuth,
    pool: Data<PgPool>,
    settings: SharedElysiaSettings,
    body: Json<ElysiaSettings>,
) -> Result<HttpResponse> {
    let new = body.into_inner();
    new.validate().map_err(Error::BadRequest)?;
    settings_repo::save(pool.get_ref(), &new).await?;
    *settings.write().expect("settings lock poisoned") = new;
    Ok(HttpResponse::Ok().json(new))
}
