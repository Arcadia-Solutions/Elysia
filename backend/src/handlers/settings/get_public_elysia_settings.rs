use actix_web::HttpResponse;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::settings::PublicElysiaSettings;
use crate::settings::SharedElysiaSettings;

#[utoipa::path(
    tag = "Settings",
    get,
    path = "/api/public-elysia-settings",
    security(("bearer" = [])),
    description = "The public subset of the settings, loaded once by the frontend. Requires a session; any logged-in user may call it.",
    responses(
        (status = 200, body = PublicElysiaSettings),
        (status = 401, description = "Missing or invalid session"),
    )
)]
pub async fn get_public_elysia_settings(
    _authenticated: AuthUser,
    settings: SharedElysiaSettings,
) -> Result<HttpResponse> {
    let current = settings.read().expect("settings lock poisoned").clone();
    Ok(HttpResponse::Ok().json(PublicElysiaSettings::from(current)))
}
