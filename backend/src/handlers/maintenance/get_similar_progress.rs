use actix_web::HttpResponse;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::similar_images::{self, SimilarProgress};

#[utoipa::path(
    tag = "Maintenance",
    get,
    path = "/api/maintenance/similar-images/progress",
    description = "Progress of the current or last similar images scan.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Scan progress", body = SimilarProgress),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn get_similar_progress(authenticated: AuthUser) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    Ok(HttpResponse::Ok().json(similar_images::progress()))
}
