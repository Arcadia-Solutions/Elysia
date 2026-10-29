use actix_web::{
    HttpResponse,
    web::{Data, Json, Path},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::handlers::images::trash_images::ImageIdsRequest;
use crate::middlewares::AuthUser;
use crate::repository::albums;
use crate::services::albums::check_can_edit;
use crate::settings::SharedElysiaSettings;

#[utoipa::path(
    tag = "Albums",
    post,
    path = "/api/albums/{id}/images",
    description = "Add images to an album, whoever uploaded them. Its owner or an admin only. \
        Unknown, trashed and already added ids are skipped.",
    params(("id" = String, Path, description = "Album id")),
    request_body = ImageIdsRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Images added"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not the album's owner, and not an admin, or albums are disabled in settings"),
        (status = 404, description = "No such album"),
    )
)]
pub async fn add_album_images(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    settings: SharedElysiaSettings,
    id: Path<Uuid>,
    body: Json<ImageIdsRequest>,
) -> Result<HttpResponse> {
    if !settings
        .read()
        .expect("settings lock poisoned")
        .albums_enabled
    {
        return Err(Error::Forbidden);
    }
    check_can_edit(pool.get_ref(), *id, &authenticated).await?;
    albums::add_images(pool.get_ref(), *id, &body.ids).await?;
    Ok(HttpResponse::NoContent().finish())
}
