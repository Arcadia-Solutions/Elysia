use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::settings as settings_repo;
use crate::settings::ElysiaSettings;
use crate::settings::SharedElysiaSettings;

#[utoipa::path(
    tag = "Settings",
    put,
    path = "/api/elysia-settings",
    request_body = ElysiaSettings,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Settings saved", body = ElysiaSettings),
        (status = 400, description = "Contradictory settings"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Authenticated but not an admin"),
    )
)]
pub async fn put_elysia_settings(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    settings: SharedElysiaSettings,
    body: Json<ElysiaSettings>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let mut new = body.into_inner();
    new.normalize();
    new.validate().map_err(Error::BadRequest)?;
    settings_repo::save(pool.get_ref(), &new).await?;
    let response = HttpResponse::Ok().json(&new);
    *settings.write().expect("settings lock poisoned") = new;
    Ok(response)
}
