use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::maintenance::{self, ScanParams};

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/maintenance/unreferenced-images/scan",
    description = "Start scanning the text-like columns of a Postgres or MySQL database in the \
        background for image, thumbnail or viewer links under the given hosts (any scheme), \
        one select per table. When it finishes, the available images uploaded before the scan \
        and linked from nowhere replace the unreferenced images list. Each select may run 10 \
        minutes at most. Follow it with `GET /api/maintenance/unreferenced-images/progress`.",
    request_body = ScanParams,
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Scan started"),
        (status = 400, description = "Unsupported database url or no hosts"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 409, description = "A scan or trash run is already going"),
    )
)]
pub async fn scan_unreferenced_images(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    body: Json<ScanParams>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    maintenance::start_scan(pool.get_ref().clone(), body.into_inner())?;
    Ok(HttpResponse::Accepted().finish())
}
