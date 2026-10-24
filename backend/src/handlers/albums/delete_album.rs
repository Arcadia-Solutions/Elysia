use actix_web::{
    HttpResponse,
    web::{Data, Path},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::repository::albums;
use crate::services::albums::check_can_edit;

#[utoipa::path(
    tag = "Albums",
    delete,
    path = "/api/albums/{id}",
    description = "Delete an album; its images are kept. Its owner or an admin only.",
    params(("id" = String, Path, description = "Album id")),
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Album deleted"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not the album's owner, and not an admin"),
        (status = 404, description = "No such album"),
    )
)]
pub async fn delete_album(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
) -> Result<HttpResponse> {
    check_can_edit(pool.get_ref(), *id, &authenticated).await?;
    albums::delete(pool.get_ref(), *id).await?;
    Ok(HttpResponse::NoContent().finish())
}
