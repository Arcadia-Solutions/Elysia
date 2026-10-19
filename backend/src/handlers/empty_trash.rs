use actix_web::{HttpResponse, web::Data};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::files;
use crate::storage::Storage;

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/images/trash/empty",
    description = "Wipe every trashed image from disk now, without waiting for its 30 days.",
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Trash emptied"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Authenticated but not an admin"),
    )
)]
pub async fn empty_trash(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    storage: Data<Storage>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    files::wipe_trash(
        pool.get_ref(),
        storage.get_ref(),
        0,
        Some(authenticated.user_id),
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
