use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::similar_images::{self, SimilarScanParams};

#[utoipa::path(
    tag = "Maintenance",
    post,
    path = "/api/maintenance/similar-images/scan",
    description = "Start grouping the available images that look alike in the background: \
        images whose perceptual hashes (computed on upload) differ in at most `max_distance` \
        bits are grouped, transitively. The groups replace the similar images list. Follow it \
        with `GET /api/maintenance/similar-images/progress`.",
    request_body = SimilarScanParams,
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Scan started"),
        (status = 400, description = "max_distance above 16"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 409, description = "A scan is already going"),
    )
)]
pub async fn scan_similar_images(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    body: Json<SimilarScanParams>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    similar_images::start_scan(pool.get_ref().clone(), body.into_inner())?;
    Ok(HttpResponse::Accepted().finish())
}
