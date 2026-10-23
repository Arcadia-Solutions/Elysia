use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::error::Result;
use crate::handlers::images::trash_images::ImageIdsRequest;
use crate::middlewares::AuthUser;
use crate::services::files;
use crate::storage::Storage;

#[utoipa::path(
    tag = "Images",
    post,
    path = "/api/images/trash/restore",
    description = "Move trashed images back out of the trash, so they are served again. \
        Unknown or not trashed ids are skipped, as are images whose bytes were uploaded \
        again since they were trashed.",
    request_body = ImageIdsRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Images restored"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Authenticated but not an admin"),
    )
)]
pub async fn restore_images(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    body: Json<ImageIdsRequest>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    files::restore(pool.get_ref(), storage.get_ref(), &body.ids).await?;
    Ok(HttpResponse::NoContent().finish())
}
