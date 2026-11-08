use actix_web::{HttpResponse, web::Data};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::files::{self, RecalculatedSizes};
use crate::storage::Storage;

#[utoipa::path(
    tag = "Maintenance",
    post,
    path = "/api/maintenance/sizes/recalculate",
    description = "Read the on-disk size of every available and trashed image and of its medium \
        and thumbnail previews, and record it. Images missing from disk are left unchanged.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Sizes recorded", body = RecalculatedSizes),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn recalculate_sizes(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    storage: Data<Storage>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let recalculated = files::recalculate_sizes(pool.get_ref(), storage.get_ref()).await?;
    Ok(HttpResponse::Ok().json(recalculated))
}
