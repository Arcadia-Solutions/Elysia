use actix_web::HttpResponse;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::maintenance::{self, UnreferencedProgress};

#[utoipa::path(
    tag = "Maintenance",
    get,
    path = "/api/maintenance/unreferenced-images/progress",
    description = "Progress of the current or last unreferenced images scan or trash run.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Run progress", body = UnreferencedProgress),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn get_unreferenced_progress(authenticated: AuthUser) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    Ok(HttpResponse::Ok().json(maintenance::progress()))
}
