use sqlx::PgPool;

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
}

pub struct FileRow {
    pub id: String,
    pub ext: String,
    pub mime: String,
}

/// Insert a file row. Ok(true) on success, Ok(false) on a unique-constraint
/// hit (either the id or the content hash) so the caller can retry with a fresh
/// id, or fall back to the existing row, before any file hits disk.
pub async fn insert(pool: &PgPool, f: &NewFile<'_>) -> Result<bool> {
    let res = sqlx::query!(
        "insert into files \
         (id, ext, mime, original_name, size, width, height, hash, \
          original_hash, original_ext, original_width, original_height, \
          requested_quality, applied_quality) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
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
    Ok(
        sqlx::query_as!(FileRow, "select id, ext, mime from files where id = $1", id)
            .fetch_optional(pool)
            .await?,
    )
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
                width, height, applied_quality \
         from files where original_hash = $1 \
           and requested_quality is not distinct from $2 \
         order by created_at limit 1",
        original_hash,
        requested_quality,
    )
    .fetch_optional(pool)
    .await?)
}

/// Output dedup: a different source that normalized to identical output bytes.
pub async fn find_record_by_hash(pool: &PgPool, hash: &str) -> Result<Option<FileRecord>> {
    Ok(sqlx::query_as!(
        FileRecord,
        "select id, ext, mime, original_ext, original_width, original_height, \
                width, height, applied_quality \
         from files where hash = $1",
        hash,
    )
    .fetch_optional(pool)
    .await?)
}
