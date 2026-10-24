use actix_web::{
    HttpResponse,
    web::{Data, Json, Path},
};
use sqlx::PgPool;
use uuid::Uuid;

use super::create_album::AlbumNameRequest;
use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::albums;
use crate::services::albums::{check_can_edit, validate_name};

#[utoipa::path(
    tag = "Albums",
    put,
    path = "/api/albums/{id}",
    description = "Rename an album. Its owner or an admin only.",
    params(("id" = String, Path, description = "Album id")),
    request_body = AlbumNameRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Album renamed"),
        (status = 400, description = "Invalid name"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not the album's owner, and not an admin"),
        (status = 404, description = "No such album"),
        (status = 409, description = "The owner already has an album with this name"),
    )
)]
pub async fn rename_album(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
    body: Json<AlbumNameRequest>,
) -> Result<HttpResponse> {
    let name = validate_name(&body.name)?;
    check_can_edit(pool.get_ref(), *id, &authenticated).await?;
    if !albums::rename(pool.get_ref(), *id, name).await? {
        return Err(Error::Conflict);
    }
    Ok(HttpResponse::NoContent().finish())
}
