use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::Result;

pub struct NewFile<'a> {
    pub id: &'a str,
    pub ext: &'a str,
    pub mime: &'a str,
    pub original_name: Option<&'a str>,
    pub size: i64,
    pub width: i32,
    pub height: i32,
    pub hash: &'a str,
    pub original_hash: &'a str,
    pub original_ext: &'a str,
    pub original_width: i32,
    pub original_height: i32,
    pub requested_quality: Option<i32>,
    pub applied_quality: Option<i32>,
    pub has_thumbnail: bool,
    pub uploaded_by_user_id: Uuid,
}

pub struct FileRow {
    pub id: String,
    pub ext: String,
    pub mime: String,
    pub has_thumbnail: bool,
    pub uploaded_by_user_id: Uuid,
}

/// Insert a file row. Ok(true) on success, Ok(false) on a unique-constraint
/// hit (either the id or the content hash) so the caller can retry with a fresh
/// id, or fall back to the existing row, before any file hits disk.
pub async fn insert(pool: &PgPool, f: &NewFile<'_>) -> Result<bool> {
    let res = sqlx::query!(
        "insert into files \
         (id, ext, mime, original_name, size, width, height, hash, \
          original_hash, original_ext, original_width, original_height, \
          requested_quality, applied_quality, has_thumbnail, uploaded_by_user_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
        f.id,
        f.ext,
        f.mime,
        f.original_name,
        f.size,
        f.width,
        f.height,
        f.hash,
        f.original_hash,
        f.original_ext,
        f.original_width,
        f.original_height,
        f.requested_quality,
        f.applied_quality,
        f.has_thumbnail,
        f.uploaded_by_user_id,
    )
    .execute(pool)
    .await;
    match res {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(false),
        Err(e) => Err(e.into()),
    }
}

pub async fn find(pool: &PgPool, id: &str) -> Result<Option<FileRow>> {
    Ok(sqlx::query_as!(
        FileRow,
        "select id, ext, mime, has_thumbnail, uploaded_by_user_id from files where id = $1",
        id
    )
    .fetch_optional(pool)
    .await?)
}

/// Bump the image (or thumbnail) view counter and return the row in one query.
pub async fn find_and_count_view(
    pool: &PgPool,
    id: &str,
    thumbnail: bool,
) -> Result<Option<FileRow>> {
    Ok(sqlx::query_as!(
        FileRow,
        "update files set \
           views = views + (not $2)::int, \
           thumbnail_views = thumbnail_views + $2::int \
         where id = $1 returning id, ext, mime, has_thumbnail, uploaded_by_user_id",
        id,
        thumbnail,
    )
    .fetch_optional(pool)
    .await?)
}

/// A stored file with its source and processing metadata.
pub struct FileRecord {
    pub id: String,
    pub ext: String,
    pub mime: String,
    pub original_ext: String,
    pub original_width: i32,
    pub original_height: i32,
    pub width: i32,
    pub height: i32,
    pub applied_quality: Option<i32>,
    pub has_thumbnail: bool,
}

/// Skip-reprocess lookup: same source bytes under the same requested quality.
/// `requested_quality` is matched with IS NOT DISTINCT FROM so NULL = NULL.
pub async fn find_record_by_source(
    pool: &PgPool,
    original_hash: &str,
    requested_quality: Option<i32>,
) -> Result<Option<FileRecord>> {
    Ok(sqlx::query_as!(
        FileRecord,
        "select id, ext, mime, original_ext, original_width, original_height, \
                width, height, applied_quality, has_thumbnail \
         from files where original_hash = $1 \
           and requested_quality is not distinct from $2 \
         order by created_at limit 1",
        original_hash,
        requested_quality,
    )
    .fetch_optional(pool)
    .await?)
}

pub struct FileMetadata {
    pub id: String,
    pub ext: String,
    pub mime: String,
    pub original_name: Option<String>,
    pub size: i64,
    pub width: i32,
    pub height: i32,
    pub original_ext: String,
    pub original_width: i32,
    pub original_height: i32,
    pub applied_quality: Option<i32>,
    pub has_thumbnail: bool,
    pub views: i64,
    pub thumbnail_views: i64,
    pub uploader_username: String,
    pub created_at: String,
}

/// Fetch a file's display metadata and its uploader's username by id.
pub async fn find_metadata(pool: &PgPool, id: &str) -> Result<Option<FileMetadata>> {
    Ok(sqlx::query_as!(
        FileMetadata,
        "select files.id, ext, mime, original_name, size, width, height, \
                original_ext, original_width, original_height, applied_quality, \
                has_thumbnail, views, thumbnail_views, users.username as uploader_username, \
                to_char(files.created_at at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') \
                  as \"created_at!\" \
         from files join users on users.id = files.uploaded_by_user_id \
         where files.id = $1",
        id
    )
    .fetch_optional(pool)
    .await?)
}

/// Move a file's upload date back to `uploaded_at` (unix seconds) when that is
/// earlier, so a deduplicated import keeps its oldest date.
pub async fn backdate_created_at(pool: &PgPool, id: &str, uploaded_at: i64) -> Result<()> {
    sqlx::query!(
        "update files set created_at = least(created_at, to_timestamp($2)) where id = $1",
        id,
        uploaded_at as f64
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// One image in a browse page, plus the total row count (same on every row,
/// from a window function, so one query returns the page and its total).
pub struct ImageListRow {
    pub id: String,
    pub ext: String,
    pub has_thumbnail: bool,
    pub total: i64,
}

/// Column the browse page is sorted by.
#[derive(Clone, Copy, Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImagesSortBy {
    #[default]
    CreatedAt,
    Size,
    Width,
    Height,
    OriginalHash,
    Views,
    ThumbnailViews,
}

impl ImagesSortBy {
    fn as_sql(self) -> &'static str {
        match self {
            ImagesSortBy::CreatedAt => "created_at",
            ImagesSortBy::Size => "size",
            ImagesSortBy::Width => "width",
            ImagesSortBy::Height => "height",
            ImagesSortBy::OriginalHash => "original_hash",
            ImagesSortBy::Views => "views",
            ImagesSortBy::ThumbnailViews => "thumbnail_views",
        }
    }
}

/// Sort direction.
#[derive(Clone, Copy, Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

/// Fetch one page of images in the given order, with the total count. `id`
/// breaks ties so rows never shift across pages.
// ponytail: CASE ordering can't use an index (full sort per page, as the window
// count scans every row anyway); add per-column queries + indexes if it gets slow.
pub async fn list_page(
    pool: &PgPool,
    offset: i64,
    limit: i64,
    sort_by: ImagesSortBy,
    order: SortOrder,
) -> Result<Vec<ImageListRow>> {
    Ok(sqlx::query_as!(
        ImageListRow,
        "select id, ext, has_thumbnail, count(*) over() as \"total!\" \
         from files order by \
           case when $3 = 'created_at' and $4 then created_at end asc, \
           case when $3 = 'created_at' and not $4 then created_at end desc, \
           case when $3 = 'size' and $4 then size end asc, \
           case when $3 = 'size' and not $4 then size end desc, \
           case when $3 = 'width' and $4 then width end asc, \
           case when $3 = 'width' and not $4 then width end desc, \
           case when $3 = 'height' and $4 then height end asc, \
           case when $3 = 'height' and not $4 then height end desc, \
           case when $3 = 'original_hash' and $4 then original_hash end asc, \
           case when $3 = 'original_hash' and not $4 then original_hash end desc, \
           case when $3 = 'views' and $4 then views end asc, \
           case when $3 = 'views' and not $4 then views end desc, \
           case when $3 = 'thumbnail_views' and $4 then thumbnail_views end asc, \
           case when $3 = 'thumbnail_views' and not $4 then thumbnail_views end desc, \
           id desc \
         offset $1 limit $2",
        offset,
        limit,
        sort_by.as_sql(),
        matches!(order, SortOrder::Asc),
    )
    .fetch_all(pool)
    .await?)
}

/// Total number of stored files; the count for an out-of-range page, where the
/// window count in `list_page` returns no rows.
pub async fn count(pool: &PgPool) -> Result<i64> {
    Ok(
        sqlx::query_scalar!("select count(*) as \"count!\" from files")
            .fetch_one(pool)
            .await?,
    )
}

/// Output dedup: a different source that normalized to identical output bytes.
pub async fn find_record_by_hash(pool: &PgPool, hash: &str) -> Result<Option<FileRecord>> {
    Ok(sqlx::query_as!(
        FileRecord,
        "select id, ext, mime, original_ext, original_width, original_height, \
                width, height, applied_quality, has_thumbnail \
         from files where hash = $1",
        hash,
    )
    .fetch_optional(pool)
    .await?)
}
