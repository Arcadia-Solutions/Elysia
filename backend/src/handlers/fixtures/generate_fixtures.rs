use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::fixtures::{self, FixturesParams};
use crate::settings::SharedElysiaSettings;
use crate::storage::Storage;

#[utoipa::path(
    tag = "Fixtures",
    post,
    path = "/api/fixtures",
    description = "Start generating fixture users and images in the background, through the \
        real upload pipeline with upload dates spread over the past. Follow it with \
        `GET /api/fixtures`.",
    request_body = FixturesParams,
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Generation started"),
        (status = 400, description = "Parameter out of range"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 409, description = "A generation is already running"),
    )
)]
pub async fn generate_fixtures(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    settings: SharedElysiaSettings,
    body: Json<FixturesParams>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let settings = settings.read().expect("settings lock poisoned").clone();
    fixtures::start(pool.get_ref().clone(), storage, settings, body.into_inner())?;
    Ok(HttpResponse::Accepted().finish())
}
