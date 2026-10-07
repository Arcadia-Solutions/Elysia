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
}

pub struct FileRow {
    pub id: String,
    pub ext: String,
    pub mime: String,
}

/// Insert a file row. Ok(true) on success, Ok(false) on id collision so the
/// caller can retry with a fresh id before any file hits disk.
pub async fn insert(pool: &PgPool, f: &NewFile<'_>) -> Result<bool> {
    let res = sqlx::query!(
        "insert into files (id, ext, mime, original_name, size, width, height) \
         values ($1, $2, $3, $4, $5, $6, $7)",
        f.id,
        f.ext,
        f.mime,
        f.original_name,
        f.size,
        f.width,
        f.height,
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
