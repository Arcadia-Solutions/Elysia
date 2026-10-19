use std::num::NonZeroU32;

use serde::Deserialize;
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
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
    /// The API key the file was uploaded with, if any.
    pub api_key_id: Option<Uuid>,
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
          requested_quality, applied_quality, has_thumbnail, uploaded_by_user_id, api_key_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
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
        f.api_key_id,
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

/// Add pending view counts, one entry per id across the three slices. Ids
/// with no row (never inserted) are skipped.
pub async fn add_views(
    pool: &PgPool,
    ids: &[String],
    views: &[i64],
    thumbnail_views: &[i64],
) -> Result<()> {
    sqlx::query!(
        "update files set \
           views = files.views + pending.views, \
           thumbnail_views = files.thumbnail_views + pending.thumbnail_views \
         from unnest($1::text[], $2::bigint[], $3::bigint[]) \
           as pending(id, views, thumbnail_views) \
         where files.id = pending.id",
        ids,
        views,
        thumbnail_views,
    )
    .execute(pool)
    .await?;
    Ok(())
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
         from files where original_hash = $1 and status = 'available' \
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
    pub uploaded_by_user_id: Uuid,
    pub uploader_username: String,
    pub created_at: String,
    /// Still within `UPLOADER_TRASH_WINDOW_MINUTES` of its upload.
    pub recent: bool,
}

/// Minutes after upload during which uploaders may trash their own files.
/// Also spelled out as "an hour" in the OpenAPI descriptions.
pub const UPLOADER_TRASH_WINDOW_MINUTES: i32 = 60;

/// Fetch an available file's display metadata and its uploader's username by
/// id; `recent` while within `UPLOADER_TRASH_WINDOW_MINUTES` of its upload.
pub async fn find_metadata(pool: &PgPool, id: &str) -> Result<Option<FileMetadata>> {
    Ok(sqlx::query_as!(
        FileMetadata,
        "select files.id, ext, mime, original_name, size, width, height, \
                original_ext, original_width, original_height, applied_quality, \
                has_thumbnail, views, thumbnail_views, uploaded_by_user_id, \
                users.username as uploader_username, \
                to_char(files.created_at at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') \
                  as \"created_at!\", \
                files.created_at > now() - make_interval(mins => $2) as \"recent!\" \
         from files join users on users.id = files.uploaded_by_user_id \
         where files.id = $1 and status = 'available'",
        id,
        UPLOADER_TRASH_WINDOW_MINUTES
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
#[derive(Clone, Copy, Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImagesSortBy {
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

/// Which images the browse page lists: a subset of the `file_status` enum.
#[derive(Clone, Copy, Debug, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "file_status", rename_all = "lowercase")]
pub enum ImagesStatus {
    Available,
    Trashed,
}

/// Sort direction.
#[derive(Clone, Copy, Debug, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

/// Browse page query.
#[derive(Clone, Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ImagesQuery {
    /// 1-based page number.
    #[param(value_type = i64, minimum = 1)]
    pub page: NonZeroU32,
    pub sort_by: ImagesSortBy,
    pub order: SortOrder,
    /// Available or trashed images.
    pub status: ImagesStatus,
    /// Admin only: just this user's uploads (exact username).
    pub username: Option<String>,
}

/// Fetch one page of images with the given status in the given order, with the total count,
/// limited to `uploader`'s files when set. `id` breaks ties so rows never shift across pages.
// ponytail: CASE ordering can't use an index (full sort per page, as the window
// count scans every row anyway); add per-column queries + indexes if it gets slow.
pub async fn list_page(
    pool: &PgPool,
    offset: i64,
    limit: i64,
    query: &ImagesQuery,
    uploader: Option<Uuid>,
) -> Result<Vec<ImageListRow>> {
    Ok(sqlx::query_as!(
        ImageListRow,
        "select id, ext, has_thumbnail, count(*) over() as \"total!\" \
         from files where status = $5 \
           and ($6::uuid is null or uploaded_by_user_id = $6) order by \
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
        query.sort_by.as_sql(),
        matches!(query.order, SortOrder::Asc),
        query.status as ImagesStatus,
        uploader,
    )
    .fetch_all(pool)
    .await?)
}

/// Number of files with the given status, limited to `uploader`'s when set; the
/// count for an out-of-range page, where the window count in `list_page` returns no rows.
pub async fn count(pool: &PgPool, status: ImagesStatus, uploader: Option<Uuid>) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        "select count(*) as \"count!\" from files where status = $1 \
           and ($2::uuid is null or uploaded_by_user_id = $2)",
        status as ImagesStatus,
        uploader
    )
    .fetch_one(pool)
    .await?)
}

/// Output dedup: a different source that normalized to identical output bytes.
pub async fn find_record_by_hash(pool: &PgPool, hash: &str) -> Result<Option<FileRecord>> {
    Ok(sqlx::query_as!(
        FileRecord,
        "select id, ext, mime, original_ext, original_width, original_height, \
                width, height, applied_quality, has_thumbnail \
         from files where hash = $1 and status = 'available'",
        hash,
    )
    .fetch_optional(pool)
    .await?)
}

/// A file the trash acts on; `recent` as in `find_metadata`.
pub struct TrashRow {
    pub id: String,
    pub ext: String,
    pub uploaded_by_user_id: Uuid,
    pub recent: bool,
}

/// The available files among `ids`; unknown or already trashed ids are left out.
pub async fn find_available(pool: &PgPool, ids: &[String]) -> Result<Vec<TrashRow>> {
    Ok(sqlx::query_as!(
        TrashRow,
        "select id, ext, uploaded_by_user_id, \
                created_at > now() - make_interval(mins => $2) as \"recent!\" \
         from files where id = any($1) and status = 'available'",
        ids,
        UPLOADER_TRASH_WINDOW_MINUTES
    )
    .fetch_all(pool)
    .await?)
}

/// Flag available files as trashed by `user_id`, starting their 30 days in the trash.
pub async fn mark_trashed(pool: &PgPool, ids: &[String], user_id: Uuid) -> Result<()> {
    sqlx::query!(
        "update files set status = 'trashed', trashed_at = now(), trashed_by_id = $2 \
         where id = any($1) and status = 'available'",
        ids,
        user_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A trashed file the wipe acts on.
pub struct TrashedRow {
    pub id: String,
    pub ext: String,
}

/// Files trashed more than `days` days ago; every trashed file for 0.
pub async fn trashed_before(pool: &PgPool, days: i32) -> Result<Vec<TrashedRow>> {
    Ok(sqlx::query_as!(
        TrashedRow,
        "select id, ext from files \
         where status = 'trashed' and trashed_at <= now() - make_interval(days => $1)",
        days
    )
    .fetch_all(pool)
    .await?)
}

/// Flag trashed files as deleted, once wiped from disk: by the admin
/// `deleted_by_id` when set, by the scheduler otherwise.
pub async fn mark_deleted(
    pool: &PgPool,
    ids: &[String],
    deleted_by_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query!(
        "update files set status = 'deleted', deleted_by_id = $2, \
           deletion_source = case when $2::uuid is null then 'scheduler' else 'manual' end::deletion_source \
         where id = any($1) and status = 'trashed'",
        ids,
        deleted_by_id as Option<Uuid>
    )
    .execute(pool)
    .await?;
    Ok(())
}
