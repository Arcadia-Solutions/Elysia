use actix_web::{HttpResponse, web::Data};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::maintenance;
use crate::storage::Storage;

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/maintenance/unreferenced-images/trash",
    description = "Start moving every image of the unreferenced images list to the trash in \
        the background. Refused once the scan is more than 24 hours old, as links added since may point to its \
        images. Follow it with `GET /api/maintenance/unreferenced-images/progress`.",
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Trashing started"),
        (status = 400, description = "The scan is too old"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 409, description = "A scan or trash run is already going"),
    )
)]
pub async fn trash_unreferenced_images(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    storage: Data<Storage>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    maintenance::start_trash(pool.get_ref().clone(), storage, authenticated.user_id).await?;
    Ok(HttpResponse::Accepted().finish())
}
