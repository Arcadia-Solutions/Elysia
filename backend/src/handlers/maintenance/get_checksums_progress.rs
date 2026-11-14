use actix_web::HttpResponse;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::checksums::{self, ChecksumsProgress};

#[utoipa::path(
    tag = "Maintenance",
    get,
    path = "/api/maintenance/checksums/progress",
    description = "Progress of the current or last checksum computation.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Computation progress", body = ChecksumsProgress),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn get_checksums_progress(authenticated: AuthUser) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    Ok(HttpResponse::Ok().json(checksums::progress()))
}
