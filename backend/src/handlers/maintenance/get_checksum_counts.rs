use actix_web::{HttpResponse, web::Data};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::checksums::{self, ChecksumCounts};

#[utoipa::path(
    tag = "Maintenance",
    get,
    path = "/api/maintenance/checksums",
    description = "How many available images have a checksum of each algorithm.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Checksum coverage", body = ChecksumCounts),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn get_checksum_counts(
    authenticated: AuthUser,
    pool: Data<PgPool>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    Ok(HttpResponse::Ok().json(checksums::counts(pool.get_ref()).await?))
}
