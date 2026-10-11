use actix_web::HttpResponse;

use crate::error::Result;
use crate::handlers::settings::SharedElysiaSettings;
use crate::settings::UnauthElysiaSettings;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/elysia-unauth-settings",
    description = "Whether registrations are open, so the login and register pages can offer signup without a session.",
    responses(
        (status = 200, body = UnauthElysiaSettings),
    )
)]
pub async fn get_elysia_unauth_settings(settings: SharedElysiaSettings) -> Result<HttpResponse> {
    let current = *settings.read().expect("settings lock poisoned");
    Ok(HttpResponse::Ok().json(UnauthElysiaSettings::from(current)))
}
