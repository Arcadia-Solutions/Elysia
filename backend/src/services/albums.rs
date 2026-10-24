use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::albums::{self, AlbumListRow, AlbumsQuery};

/// Longest accepted album name.
const MAXIMUM_NAME_LENGTH: usize = 64;

/// The trimmed album name, refused unless 1 to 64 characters long.
pub fn validate_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAXIMUM_NAME_LENGTH {
        return Err(Error::BadRequest(
            "name must be between 1 and 64 characters".into(),
        ));
    }
    Ok(name)
}

/// Check that the album exists and `user` may change it: its owner or an admin.
/// Anyone may view it.
pub async fn check_can_edit(pool: &PgPool, id: Uuid, user: &AuthUser) -> Result<()> {
    match albums::find_owner(pool, id).await? {
        Some(owner_id) if user.is_admin || owner_id == user.user_id => Ok(()),
        Some(_) => Err(Error::Forbidden),
        None => Err(Error::NotFound),
    }
}

/// Fetch one page (1-based) of albums, only `owner`'s when set, with the total count.
pub async fn list_albums(
    pool: &PgPool,
    query: &AlbumsQuery,
    owner: Option<Uuid>,
) -> Result<(Vec<AlbumListRow>, i64)> {
    super::paginate(
        query.page,
        query.page_size,
        |offset, limit| albums::list_page(pool, offset, limit, query.sort_by, query.order, owner),
        albums::count(pool, owner),
        |row| row.total,
    )
    .await
}
