use actix_web::{
    HttpResponse,
    web::{Data, Json, Path},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;
use crate::handlers::images::trash_images::ImageIdsRequest;
use crate::middlewares::AuthUser;
use crate::repository::albums;
use crate::services::albums::check_can_edit;

#[utoipa::path(
    tag = "Albums",
    post,
    path = "/api/albums/{id}/images/remove",
    description = "Remove images from an album; the images themselves are kept. Its owner or an \
        admin only. Ids not in the album are skipped.",
    params(("id" = String, Path, description = "Album id")),
    request_body = ImageIdsRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Images removed"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not the album's owner, and not an admin"),
        (status = 404, description = "No such album"),
    )
)]
pub async fn remove_album_images(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
    body: Json<ImageIdsRequest>,
) -> Result<HttpResponse> {
    check_can_edit(pool.get_ref(), *id, &authenticated).await?;
    albums::remove_images(pool.get_ref(), *id, &body.ids).await?;
    Ok(HttpResponse::NoContent().finish())
}
