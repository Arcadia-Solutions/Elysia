use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::checksums::{self, ComputeChecksumsParams};
use crate::storage::Storage;

#[utoipa::path(
    tag = "Maintenance",
    post,
    path = "/api/maintenance/checksums/compute",
    description = "Start computing the given checksums of the available images missing \
        them in the background, so their `/ih/`, `/mh/` and `/th/` links work. Images \
        missing from disk are skipped. Follow it with \
        `GET /api/maintenance/checksums/progress`.",
    request_body = ComputeChecksumsParams,
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Computation started"),
        (status = 400, description = "No algorithm given"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 409, description = "A computation is already going"),
    )
)]
pub async fn compute_checksums(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    body: Json<ComputeChecksumsParams>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    checksums::start_compute(pool.get_ref().clone(), storage, body.into_inner())?;
    Ok(HttpResponse::Accepted().finish())
}
