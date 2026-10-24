use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::albums::{self, Album};
use crate::services::albums::validate_name;

#[derive(Deserialize, ToSchema)]
pub struct AlbumNameRequest {
    /// 1 to 64 characters, unique among the owner's albums.
    pub name: String,
}

#[utoipa::path(
    tag = "Albums",
    post,
    path = "/api/albums",
    description = "Create an album owned by the caller.",
    request_body = AlbumNameRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Album created", body = Album),
        (status = 400, description = "Invalid name"),
        (status = 401, description = "Missing or invalid session"),
        (status = 409, description = "The caller already has an album with this name"),
    )
)]
pub async fn create_album(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    body: Json<AlbumNameRequest>,
) -> Result<HttpResponse> {
    let name = validate_name(&body.name)?;
    let album = albums::create(pool.get_ref(), authenticated.user_id, name)
        .await?
        .ok_or(Error::Conflict)?;
    Ok(HttpResponse::Ok().json(album))
}
