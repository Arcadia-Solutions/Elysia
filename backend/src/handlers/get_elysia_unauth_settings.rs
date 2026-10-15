use actix_web::{HttpResponse, web::Data};

use crate::config::Config;
use crate::error::Result;
use crate::settings::SharedElysiaSettings;
use crate::settings::UnauthElysiaSettings;

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/elysia-unauth-settings",
    description = "Whether registrations are open and SSO is enabled, so the login and register pages can offer signup and single sign-on without a session.",
    responses(
        (status = 200, body = UnauthElysiaSettings),
    )
)]
pub async fn get_elysia_unauth_settings(
    settings: SharedElysiaSettings,
    config: Data<Config>,
) -> Result<HttpResponse> {
    let current = settings.read().expect("settings lock poisoned").clone();
    Ok(HttpResponse::Ok().json(UnauthElysiaSettings {
        allow_registration: current.allow_registration,
        sso_provider_name: config.auth.sso.as_ref().map(|sso| sso.name.clone()),
    }))
}
