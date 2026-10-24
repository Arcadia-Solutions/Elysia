use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::albums::AlbumsQuery;
use crate::repository::{PaginatedResults, users};
use crate::services::albums;

/// One album in the albums list.
#[derive(Serialize, ToSchema)]
pub struct AlbumSummary {
    #[schema(value_type = String)]
    pub id: Uuid,
    pub name: String,
    pub owner_username: String,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// Available images in the album.
    pub image_count: i64,
}

#[utoipa::path(
    tag = "Albums",
    get,
    path = "/api/albums",
    description = "Admins list all albums, or one user's with `username`, other users only their own. \
        `mine` lists only the caller's albums, also for admins.",
    params(AlbumsQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of albums", body = PaginatedResults<AlbumSummary>),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin, and filtered by username"),
    )
)]
pub async fn list_albums(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    query: Query<AlbumsQuery>,
) -> Result<HttpResponse> {
    let owner = match (authenticated.is_admin, query.mine, &query.username) {
        (_, true, _) | (false, false, None) => Some(authenticated.user_id),
        (true, false, None) => None,
        // An unknown user owns nothing: the nil id matches no album.
        (true, false, Some(username)) => Some(
            users::find_id_by_username(pool.get_ref(), username)
                .await?
                .unwrap_or_default(),
        ),
        (false, false, Some(_)) => return Err(Error::Forbidden),
    };
    let (rows, total) = albums::list_albums(pool.get_ref(), &query, owner).await?;
    let results = rows
        .into_iter()
        .map(|row| AlbumSummary {
            id: row.id,
            name: row.name,
            owner_username: row.owner_username,
            created_at: row.created_at,
            image_count: row.image_count,
        })
        .collect();
    Ok(HttpResponse::Ok().json(PaginatedResults::new(
        results,
        query.page,
        query.page_size,
        total,
    )))
}
