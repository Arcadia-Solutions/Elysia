use actix_web::HttpResponse;

use crate::error::Result;
use crate::handlers::settings::SharedElysiaSettings;
use crate::middlewares::AuthUser;
use crate::settings::ElysiaSettings;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/elysia-settings",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Current elysia settings", body = ElysiaSettings),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Authenticated but not an admin"),
    )
)]
pub async fn get_elysia_settings(
    authenticated: AuthUser,
    settings: SharedElysiaSettings,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let current = *settings.read().expect("settings lock poisoned");
    Ok(HttpResponse::Ok().json(current))
}
