use actix_web::HttpResponse;

use crate::error::Result;
use crate::handlers::settings::SharedElysiaSettings;
use crate::middlewares::AuthUser;
use crate::settings::PublicElysiaSettings;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/public-elysia-settings",
    security(("bearer" = [])),
    description = "The public subset of the settings, so the upload page can validate files client-side before sending them. Requires a session; any logged-in user may call it.",
    responses(
        (status = 200, body = PublicElysiaSettings),
        (status = 401, description = "Missing or invalid session"),
    )
)]
pub async fn get_public_elysia_settings(
    _authenticated: AuthUser,
    settings: SharedElysiaSettings,
) -> Result<HttpResponse> {
    let current = *settings.read().expect("settings lock poisoned");
    Ok(HttpResponse::Ok().json(PublicElysiaSettings::from(current)))
}
