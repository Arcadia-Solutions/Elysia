use actix_web::HttpResponse;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::chevereto_import;

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/maintenance/chevereto-import/stop",
    description = "Stop the running Chevereto import once its current chunk is saved. Start it \
        again with `resume` to continue.",
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Stop requested"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 409, description = "No import is running"),
    )
)]
pub async fn stop_chevereto_import(authenticated: AuthUser) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    chevereto_import::stop()?;
    Ok(HttpResponse::Accepted().finish())
}
