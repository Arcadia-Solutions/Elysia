use actix_web::HttpResponse;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::fixtures::{self, FixturesProgress};

#[utoipa::path(
    tag = "Fixtures",
    get,
    path = "/api/fixtures",
    description = "Progress of the current or last fixtures generation.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Generation progress", body = FixturesProgress),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn get_fixtures_progress(authenticated: AuthUser) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    Ok(HttpResponse::Ok().json(fixtures::progress()))
}
