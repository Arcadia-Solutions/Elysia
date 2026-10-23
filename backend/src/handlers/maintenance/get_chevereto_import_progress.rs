use actix_web::{HttpResponse, web::Data};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::chevereto_import::{self, CheveretoImportProgress};

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/maintenance/chevereto-import/progress",
    description = "Progress of the current or last Chevereto import.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Import progress", body = CheveretoImportProgress),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn get_chevereto_import_progress(
    authenticated: AuthUser,
    pool: Data<PgPool>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    Ok(HttpResponse::Ok().json(chevereto_import::progress(pool.get_ref()).await?))
}
