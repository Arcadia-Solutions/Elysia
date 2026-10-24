use actix_web::{
    HttpResponse,
    web::{Data, Path},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::albums::{self, Album};

#[utoipa::path(
    tag = "Albums",
    get,
    path = "/api/albums/{id}",
    description = "Any album's details; its images are listed by `/api/images` with `album_id`.",
    params(("id" = String, Path, description = "Album id")),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The album", body = Album),
        (status = 401, description = "Missing or invalid session"),
        (status = 404, description = "No such album"),
    )
)]
pub async fn get_album(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
) -> Result<HttpResponse> {
    let album = albums::find(pool.get_ref(), *id, authenticated.user_id)
        .await?
        .ok_or(Error::NotFound)?;
    Ok(HttpResponse::Ok().json(album))
}
