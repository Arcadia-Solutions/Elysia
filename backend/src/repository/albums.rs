use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::Result;
use crate::repository::files::SortOrder;

/// An album, without its images.
#[derive(Serialize, ToSchema)]
pub struct Album {
    #[schema(value_type = String)]
    pub id: Uuid,
    pub name: String,
    pub owner_username: String,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// The caller's own album.
    pub mine: bool,
}

/// Create an album. None when the owner already has an album with this name.
pub async fn create(pool: &PgPool, owner_id: Uuid, name: &str) -> Result<Option<Album>> {
    let res = sqlx::query_as!(
        Album,
        r#"with album as (insert into albums (owner_id, name) values ($1, $2) returning *)
         select album.id, album.name, users.username as owner_username,
                to_char(album.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "created_at!",
                true as "mine!"
         from album join users on users.id = album.owner_id"#,
        owner_id,
        name
    )
    .fetch_one(pool)
    .await;
    match res {
        Ok(album) => Ok(Some(album)),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// An album, `mine` when `caller` owns it.
pub async fn find(pool: &PgPool, id: Uuid, caller: Uuid) -> Result<Option<Album>> {
    Ok(sqlx::query_as!(
        Album,
        r#"select albums.id, albums.name, users.username as owner_username,
                to_char(albums.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "created_at!",
                albums.owner_id = $2 as "mine!"
         from albums join users on users.id = albums.owner_id where albums.id = $1"#,
        id,
        caller
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn find_owner(pool: &PgPool, id: Uuid) -> Result<Option<Uuid>> {
    Ok(
        sqlx::query_scalar!(r#"select owner_id from albums where id = $1"#, id)
            .fetch_optional(pool)
            .await?,
    )
}

/// Rename an album. False when its owner already has an album with this name.
pub async fn rename(pool: &PgPool, id: Uuid, name: &str) -> Result<bool> {
    let res = sqlx::query!(r#"update albums set name = $2 where id = $1"#, id, name)
        .execute(pool)
        .await;
    match res {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Delete an album; its images stay.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query!(r#"delete from albums where id = $1"#, id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Add the available files among `ids` to an album; unknown, trashed and
/// already added ids are skipped.
pub async fn add_images(pool: &PgPool, id: Uuid, ids: &[String]) -> Result<()> {
    sqlx::query!(
        r#"insert into album_images (album_id, file_id)
         select $1, files.id from files where files.id = any($2) and status = 'available'
         on conflict do nothing"#,
        id,
        ids
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_images(pool: &PgPool, id: Uuid, ids: &[String]) -> Result<()> {
    sqlx::query!(
        r#"delete from album_images where album_id = $1 and file_id = any($2)"#,
        id,
        ids
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// An album an image is in, as listed on the image page.
#[derive(Serialize, ToSchema)]
pub struct ImageAlbum {
    #[schema(value_type = String)]
    pub id: Uuid,
    pub name: String,
    pub owner_username: String,
    /// The caller's own album, which they may remove the image from.
    pub mine: bool,
}

/// Every album a file is in, `caller`'s first, then by name.
pub async fn for_file(pool: &PgPool, file_id: &str, caller: Uuid) -> Result<Vec<ImageAlbum>> {
    Ok(sqlx::query_as!(
        ImageAlbum,
        r#"select albums.id, albums.name, users.username as owner_username,
                albums.owner_id = $2 as "mine!"
         from album_images
         join albums on albums.id = album_images.album_id
         join users on users.id = albums.owner_id
         where album_images.file_id = $1
         order by albums.owner_id = $2 desc, albums.name, users.username"#,
        file_id,
        caller
    )
    .fetch_all(pool)
    .await?)
}

/// One album in the albums list, plus the total row count (window function, as
/// in `files::list_page`).
pub struct AlbumListRow {
    pub id: Uuid,
    pub name: String,
    pub owner_username: String,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// Available images only.
    pub image_count: i64,
    pub total: i64,
}

/// Column the albums list is sorted by.
#[derive(Clone, Copy, Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlbumsSortBy {
    Name,
    OwnerUsername,
    CreatedAt,
    ImageCount,
}

impl AlbumsSortBy {
    fn as_sql(self) -> &'static str {
        match self {
            AlbumsSortBy::Name => "name",
            AlbumsSortBy::OwnerUsername => "owner_username",
            AlbumsSortBy::CreatedAt => "created_at",
            AlbumsSortBy::ImageCount => "image_count",
        }
    }
}

/// Albums list query.
#[derive(Clone, Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AlbumsQuery {
    /// 1-based page number.
    #[param(value_type = i64, minimum = 1)]
    pub page: NonZeroU32,
    #[serde(deserialize_with = "crate::repository::page_size")]
    #[param(minimum = 1, maximum = 500)]
    pub page_size: u32,
    pub sort_by: AlbumsSortBy,
    pub order: SortOrder,
    /// Admin only: just this user's albums (exact username).
    pub username: Option<String>,
    /// Just the caller's albums, also for admins.
    #[serde(default)]
    pub mine: bool,
}

/// One page of albums in the given order, only `owner`'s when set. `id` breaks ties.
// ponytail: counts every album's images per page, fine for hand-made albums;
// keep a count column if albums get huge.
pub async fn list_page(
    pool: &PgPool,
    offset: i64,
    limit: i64,
    sort_by: AlbumsSortBy,
    order: SortOrder,
    owner: Option<Uuid>,
) -> Result<Vec<AlbumListRow>> {
    Ok(sqlx::query_as!(
        AlbumListRow,
        r#"select a.id, a.name, u.username as owner_username,
                to_char(a.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "created_at!",
                stats.image_count as "image_count!",
                count(*) over() as "total!"
         from albums a
         join users u on u.id = a.owner_id
         cross join lateral (
           select count(*) as image_count from album_images ai
           join files f on f.id = ai.file_id
           where ai.album_id = a.id and f.status = 'available'
         ) stats
         where $5::uuid is null or a.owner_id = $5 order by
           case when $3 = 'name' and $4 then a.name end asc,
           case when $3 = 'name' and not $4 then a.name end desc,
           case when $3 = 'owner_username' and $4 then u.username end asc,
           case when $3 = 'owner_username' and not $4 then u.username end desc,
           case when $3 = 'created_at' and $4 then a.created_at end asc,
           case when $3 = 'created_at' and not $4 then a.created_at end desc,
           case when $3 = 'image_count' and $4 then stats.image_count end asc,
           case when $3 = 'image_count' and not $4 then stats.image_count end desc,
           a.id desc
         offset $1 limit $2"#,
        offset,
        limit,
        sort_by.as_sql(),
        matches!(order, SortOrder::Asc),
        owner,
    )
    .fetch_all(pool)
    .await?)
}

/// Number of albums, only `owner`'s when set; the count for an out-of-range page.
pub async fn count(pool: &PgPool, owner: Option<Uuid>) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"select count(*) as "count!" from albums where $1::uuid is null or owner_id = $1"#,
        owner
    )
    .fetch_one(pool)
    .await?)
}
