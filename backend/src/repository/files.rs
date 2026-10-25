use std::num::NonZeroU32;

use serde::Deserialize;
use sqlx::{PgConnection, PgPool};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::Result;
use crate::services::files::TRASH_RETENTION_DAYS;

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
    /// Perceptual hash, see `image::perceptual_hash`.
    pub phash: i64,
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
        r#"insert into files
         (id, ext, mime, original_name, size, width, height, hash,
          original_hash, original_ext, original_width, original_height,
          requested_quality, applied_quality, has_thumbnail, phash, uploaded_by_user_id, api_key_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)"#,
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
        f.phash,
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
        r#"select id, ext, mime, has_thumbnail, uploaded_by_user_id from files where id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?)
}

/// Add pending view counts, one `(id, referrer)` entry per index across the
/// slices, to the file totals and the per-file referrer counts. Ids with no row
/// (never inserted) are skipped.
pub async fn add_views(
    pool: &PgPool,
    ids: &[String],
    referrers: &[String],
    views: &[i64],
    thumbnail_views: &[i64],
) -> Result<()> {
    sqlx::query!(
        r#"with pending as (
           select pending.*
           from unnest($1::text[], $2::text[], $3::bigint[], $4::bigint[])
             as pending(id, referrer, views, thumbnail_views)
           join files on files.id = pending.id
         ), totals as (
           update files set
             views = files.views + per_file.views,
             thumbnail_views = files.thumbnail_views + per_file.thumbnail_views
           from (select id, sum(views)::bigint as views,
                        sum(thumbnail_views)::bigint as thumbnail_views
                 from pending group by id) as per_file
           where files.id = per_file.id
         )
         insert into file_referrer_views (file_id, referrer, views, thumbnail_views)
         select id, referrer, sum(views)::bigint, sum(thumbnail_views)::bigint
         from pending group by 1, 2
         on conflict (file_id, referrer) do update set
           views = file_referrer_views.views + excluded.views,
           thumbnail_views = file_referrer_views.thumbnail_views + excluded.thumbnail_views"#,
        ids,
        referrers,
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
        r#"select id, ext, mime, original_ext, original_width, original_height,
                width, height, applied_quality, has_thumbnail
         from files where original_hash = $1 and status = 'available'
           and requested_quality is not distinct from $2
         order by created_at limit 1"#,
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
    /// When the file was trashed, as a UTC ISO-8601 string. None while available.
    pub trashed_at: Option<String>,
    pub trashed_by_username: Option<String>,
    /// Whole days left before the scheduled wipe, rounded up. None while available.
    pub days_until_deletion: Option<i32>,
}

/// Minutes after upload during which uploaders may trash their own files.
/// Also spelled out as "an hour" in the OpenAPI descriptions.
pub const UPLOADER_TRASH_WINDOW_MINUTES: i32 = 60;

/// Fetch an available or trashed file's display metadata, with its uploader's
/// and trasher's usernames, by id; `recent` while within
/// `UPLOADER_TRASH_WINDOW_MINUTES` of its upload.
pub async fn find_metadata(pool: &PgPool, id: &str) -> Result<Option<FileMetadata>> {
    Ok(sqlx::query_as!(
        FileMetadata,
        r#"select files.id, ext, mime, original_name, size, width, height,
                original_ext, original_width, original_height, applied_quality,
                has_thumbnail, views, thumbnail_views, uploaded_by_user_id,
                users.username as uploader_username,
                to_char(files.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "created_at!",
                files.created_at > now() - make_interval(mins => $2) as "recent!",
                to_char(files.trashed_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as trashed_at,
                trashers.username as "trashed_by_username?",
                greatest(0, ceil(extract(epoch from
                  files.trashed_at + make_interval(days => $3) - now()) / 86400))::int
                  as days_until_deletion
         from files join users on users.id = files.uploaded_by_user_id
         left join users trashers on trashers.id = files.trashed_by_id
         where files.id = $1 and status in ('available', 'trashed')"#,
        id,
        UPLOADER_TRASH_WINDOW_MINUTES,
        TRASH_RETENTION_DAYS
    )
    .fetch_optional(pool)
    .await?)
}

/// Views of a file from one `Referer` host.
#[derive(serde::Serialize, ToSchema)]
pub struct ReferrerViews {
    /// Lowercased `Referer` host; empty for requests without a `Referer`.
    pub referrer: String,
    pub views: i64,
    pub thumbnail_views: i64,
}

/// A file's views per referrer, most image views first.
pub async fn find_referrer_views(pool: &PgPool, id: &str) -> Result<Vec<ReferrerViews>> {
    Ok(sqlx::query_as!(
        ReferrerViews,
        r#"select referrer, views, thumbnail_views from file_referrer_views
         where file_id = $1 order by views desc, thumbnail_views desc, referrer"#,
        id
    )
    .fetch_all(pool)
    .await?)
}

/// Move a file's upload date back to `uploaded_at` (unix seconds) when that is
/// earlier, so a deduplicated import keeps its oldest date.
pub async fn backdate_created_at(pool: &PgPool, id: &str, uploaded_at: i64) -> Result<()> {
    sqlx::query!(
        r#"update files set created_at = least(created_at, to_timestamp($2)) where id = $1"#,
        id,
        uploaded_at as f64
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// One image in a browse or unreferenced images page, plus the total row count
/// (same on every row, from a window function, so one query returns the page and its total).
pub struct ImageListRow {
    pub id: String,
    pub ext: String,
    pub original_name: Option<String>,
    pub size: i64,
    pub width: i32,
    pub height: i32,
    pub has_thumbnail: bool,
    pub views: i64,
    pub uploader_username: String,
    /// RFC 3339, UTC.
    pub created_at: String,
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
    #[serde(deserialize_with = "crate::repository::page_size")]
    #[param(minimum = 1, maximum = 500)]
    pub page_size: u32,
    pub sort_by: ImagesSortBy,
    pub order: SortOrder,
    /// Available or trashed images.
    pub status: ImagesStatus,
    /// Admin only: just this user's uploads (exact username).
    pub username: Option<String>,
    /// Just this album's images, whoever uploaded them.
    #[param(value_type = Option<String>)]
    pub album_id: Option<Uuid>,
}

/// Fetch one page of images with the given status in the given order, with the total count,
/// limited to `uploader`'s files and to the query's album when set. `id` breaks ties so rows never shift across pages.
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
        r#"select f.id, f.ext, f.original_name, f.size, f.width, f.height, f.has_thumbnail,
                f.views, u.username as uploader_username,
                to_char(f.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "created_at!",
                count(*) over() as "total!"
         from files f join users u on u.id = f.uploaded_by_user_id
         where f.status = $5
           and ($6::uuid is null or f.uploaded_by_user_id = $6)
           and ($7::uuid is null or exists (select 1 from album_images
                                            where album_id = $7 and file_id = f.id)) order by
           case when $3 = 'created_at' and $4 then f.created_at end asc,
           case when $3 = 'created_at' and not $4 then f.created_at end desc,
           case when $3 = 'size' and $4 then f.size end asc,
           case when $3 = 'size' and not $4 then f.size end desc,
           case when $3 = 'width' and $4 then f.width end asc,
           case when $3 = 'width' and not $4 then f.width end desc,
           case when $3 = 'height' and $4 then f.height end asc,
           case when $3 = 'height' and not $4 then f.height end desc,
           case when $3 = 'original_hash' and $4 then f.original_hash end asc,
           case when $3 = 'original_hash' and not $4 then f.original_hash end desc,
           case when $3 = 'views' and $4 then f.views end asc,
           case when $3 = 'views' and not $4 then f.views end desc,
           case when $3 = 'thumbnail_views' and $4 then f.thumbnail_views end asc,
           case when $3 = 'thumbnail_views' and not $4 then f.thumbnail_views end desc,
           f.id desc
         offset $1 limit $2"#,
        offset,
        limit,
        query.sort_by.as_sql(),
        matches!(query.order, SortOrder::Asc),
        query.status as ImagesStatus,
        uploader,
        query.album_id,
    )
    .fetch_all(pool)
    .await?)
}

/// One page of the last unreferenced images scan's images that are still
/// available, in the given order, like `list_page`.
pub async fn unreferenced_page(
    pool: &PgPool,
    offset: i64,
    limit: i64,
    sort_by: ImagesSortBy,
    order: SortOrder,
) -> Result<Vec<ImageListRow>> {
    Ok(sqlx::query_as!(
        ImageListRow,
        r#"select f.id, f.ext, f.original_name, f.size, f.width, f.height, f.has_thumbnail,
                f.views, u.username as uploader_username,
                to_char(f.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "created_at!",
                count(*) over() as "total!"
         from files f
         join unreferenced_files on file_id = f.id
         join users u on u.id = f.uploaded_by_user_id
         where f.status = 'available' order by
           case when $3 = 'created_at' and $4 then f.created_at end asc,
           case when $3 = 'created_at' and not $4 then f.created_at end desc,
           case when $3 = 'size' and $4 then f.size end asc,
           case when $3 = 'size' and not $4 then f.size end desc,
           case when $3 = 'width' and $4 then f.width end asc,
           case when $3 = 'width' and not $4 then f.width end desc,
           case when $3 = 'height' and $4 then f.height end asc,
           case when $3 = 'height' and not $4 then f.height end desc,
           case when $3 = 'original_hash' and $4 then f.original_hash end asc,
           case when $3 = 'original_hash' and not $4 then f.original_hash end desc,
           case when $3 = 'views' and $4 then f.views end asc,
           case when $3 = 'views' and not $4 then f.views end desc,
           case when $3 = 'thumbnail_views' and $4 then f.thumbnail_views end asc,
           case when $3 = 'thumbnail_views' and not $4 then f.thumbnail_views end desc,
           f.id desc
         offset $1 limit $2"#,
        offset,
        limit,
        sort_by.as_sql(),
        matches!(order, SortOrder::Asc),
    )
    .fetch_all(pool)
    .await?)
}

/// Number of the last unreferenced images scan's images still available; the
/// count for an out-of-range page, where `unreferenced_page` returns no rows.
pub async fn unreferenced_count(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"select count(*) as "count!" from files
         join unreferenced_files on file_id = id where status = 'available'"#
    )
    .fetch_one(pool)
    .await?)
}

/// Replace the unreferenced images scan result with the available files
/// uploaded before `before` (unix seconds) whose id is not in `referenced`.
/// Returns how many there are.
pub async fn replace_unreferenced(
    pool: &PgPool,
    referenced: &[String],
    before: f64,
) -> Result<u64> {
    let mut transaction = pool.begin().await?;
    sqlx::query!(r#"delete from unreferenced_files"#)
        .execute(&mut *transaction)
        .await?;
    // `not in` a subquery is hashed, `<> all(array)` scans the array per row.
    let inserted = sqlx::query!(
        r#"insert into unreferenced_files (file_id, scanned_at)
         select id, to_timestamp($2) from files where status = 'available'
           and created_at < to_timestamp($2) and id not in (select unnest($1::text[]))"#,
        referenced,
        before
    )
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    transaction.commit().await?;
    Ok(inserted)
}

/// When the last unreferenced images scan started.
pub struct UnreferencedScanRow {
    /// RFC 3339, UTC.
    pub scanned_at: String,
    /// Whether it started less than `max_age_hours` ago.
    pub recent: bool,
}

/// The last unreferenced images scan, None when it found nothing.
pub async fn unreferenced_scan(
    pool: &PgPool,
    max_age_hours: i32,
) -> Result<Option<UnreferencedScanRow>> {
    Ok(sqlx::query_as!(
        UnreferencedScanRow,
        r#"select to_char(min(scanned_at) at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "scanned_at!",
                min(scanned_at) > now() - make_interval(hours => $1) as "recent!"
         from unreferenced_files having count(*) > 0"#,
        max_age_hours
    )
    .fetch_optional(pool)
    .await?)
}

/// Ids of the last scan's unreferenced images that are still available.
pub async fn unreferenced_ids(pool: &PgPool) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(
        r#"select id from files join unreferenced_files on file_id = id
         where status = 'available' order by id"#
    )
    .fetch_all(pool)
    .await?)
}

/// An available file's perceptual hash.
pub struct PhashRow {
    pub id: String,
    pub phash: i64,
}

/// Every available file's perceptual hash, by id.
pub async fn phashes(pool: &PgPool) -> Result<Vec<PhashRow>> {
    Ok(sqlx::query_as!(
        PhashRow,
        r#"select id, phash from files where status = 'available' order by id"#
    )
    .fetch_all(pool)
    .await?)
}

/// Replace the similar images scan result: `ids[i]` is in group `group_ids[i]`.
/// Returns when it was replaced (RFC 3339, UTC).
pub async fn replace_similar(pool: &PgPool, ids: &[String], group_ids: &[i32]) -> Result<String> {
    let mut transaction = pool.begin().await?;
    sqlx::query!(r#"delete from similar_files"#)
        .execute(&mut *transaction)
        .await?;
    sqlx::query!(
        r#"insert into similar_files (file_id, group_id)
         select * from unnest($1::text[], $2::int[])"#,
        ids,
        group_ids
    )
    .execute(&mut *transaction)
    .await?;
    let replaced_at = sqlx::query_scalar!(
        r#"select to_char(now() at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as "replaced_at!""#
    )
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(replaced_at)
}

/// The images of one page of the last similar images scan's groups, by group,
/// counting only groups still holding two available images and listing only
/// those. The largest image of a group comes first. Each row is its group id
/// and the image, whose `total` counts groups, not images.
pub async fn similar_page(
    pool: &PgPool,
    offset: i64,
    limit: i64,
) -> Result<Vec<(i32, ImageListRow)>> {
    let rows = sqlx::query!(
        r#"with groups as (
           select group_id from similar_files join files on id = file_id
           where status = 'available' group by group_id having count(*) > 1
         ), page as (
           select group_id, count(*) over() as total from groups
           order by group_id offset $1 limit $2
         )
         select page.group_id, page.total as "total!",
                f.id, f.ext, f.original_name, f.size, f.width, f.height, f.has_thumbnail,
                f.views, u.username as uploader_username,
                to_char(f.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "created_at!"
         from page
         join similar_files s on s.group_id = page.group_id
         join files f on f.id = s.file_id
         join users u on u.id = f.uploaded_by_user_id
         where f.status = 'available'
         order by page.group_id, f.width::bigint * f.height desc, f.size desc, f.id"#,
        offset,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.group_id,
                ImageListRow {
                    id: row.id,
                    ext: row.ext,
                    original_name: row.original_name,
                    size: row.size,
                    width: row.width,
                    height: row.height,
                    has_thumbnail: row.has_thumbnail,
                    views: row.views,
                    uploader_username: row.uploader_username,
                    created_at: row.created_at,
                    total: row.total,
                },
            )
        })
        .collect())
}

/// Number of the last similar images scan's groups still holding two available
/// images; the count for an out-of-range page, where `similar_page` returns no rows.
pub async fn similar_group_count(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"select count(*) as "count!" from (
           select group_id from similar_files join files on id = file_id
           where status = 'available' group by group_id having count(*) > 1
         ) as groups"#
    )
    .fetch_one(pool)
    .await?)
}

/// Number of files with the given status, limited to `uploader`'s and to `album_id`'s
/// when set; the count for an out-of-range page, where the window count in `list_page`
/// returns no rows.
pub async fn count(
    pool: &PgPool,
    status: ImagesStatus,
    uploader: Option<Uuid>,
    album_id: Option<Uuid>,
) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"select count(*) as "count!" from files where status = $1
           and ($2::uuid is null or uploaded_by_user_id = $2)
           and ($3::uuid is null or exists (select 1 from album_images
                                            where album_id = $3 and file_id = files.id))"#,
        status as ImagesStatus,
        uploader,
        album_id
    )
    .fetch_one(pool)
    .await?)
}

/// Output dedup: a different source that normalized to identical output bytes.
pub async fn find_record_by_hash(pool: &PgPool, hash: &str) -> Result<Option<FileRecord>> {
    Ok(sqlx::query_as!(
        FileRecord,
        r#"select id, ext, mime, original_ext, original_width, original_height,
                width, height, applied_quality, has_thumbnail
         from files where hash = $1 and status = 'available'"#,
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
        r#"select id, ext, uploaded_by_user_id,
                created_at > now() - make_interval(mins => $2) as "recent!"
         from files where id = any($1) and status = 'available'"#,
        ids,
        UPLOADER_TRASH_WINDOW_MINUTES
    )
    .fetch_all(pool)
    .await?)
}

/// Flag available files as trashed by `user_id`, starting their 30 days in the trash.
pub async fn mark_trashed(pool: &PgPool, ids: &[String], user_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"update files set status = 'trashed', trashed_at = now(), trashed_by_id = $2
         where id = any($1) and status = 'available'"#,
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
        r#"select id, ext from files
         where status = 'trashed' and trashed_at <= now() - make_interval(days => $1)"#,
        days
    )
    .fetch_all(pool)
    .await?)
}

/// Flag the trashed files among `ids` that can be served again as available,
/// returning them: one per hash, none past `TRASH_RETENTION_DAYS` (the wipe is
/// due), and none whose bytes are available again (re-uploaded), so dedup
/// stays unique. A concurrent restore of the same bytes waits on
/// `files_hash_idx` and fails once this transaction commits.
pub async fn mark_restored(
    connection: &mut PgConnection,
    ids: &[String],
) -> Result<Vec<TrashedRow>> {
    Ok(sqlx::query_as!(
        TrashedRow,
        r#"update files set status = 'available', trashed_at = null, trashed_by_id = null
         where id in (select distinct on (hash) id from files trashed
                      where id = any($1) and status = 'trashed'
                        and trashed_at > now() - make_interval(days => $2)
                        and not exists (select 1 from files available
                                        where available.hash = trashed.hash
                                          and available.status = 'available')
                      order by hash, trashed_at desc)
         returning id, ext"#,
        ids,
        TRASH_RETENTION_DAYS
    )
    .fetch_all(connection)
    .await?)
}

/// Flag trashed files as deleted, once wiped from disk: by the user
/// `deleted_by_id` when set, by the scheduler otherwise. Drops their reports.
pub async fn mark_deleted(
    pool: &PgPool,
    ids: &[String],
    deleted_by_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query!(
        r#"with deleted as (
           update files set status = 'deleted', deleted_by_id = $2,
             deletion_source = case when $2::uuid is null then 'scheduler' else 'manual' end::deletion_source
           where id = any($1) and status = 'trashed'
           returning id
         )
         delete from reports where file_id in (select id from deleted)"#,
        ids,
        deleted_by_id as Option<Uuid>
    )
    .execute(pool)
    .await?;
    Ok(())
}
