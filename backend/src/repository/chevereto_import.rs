use sqlx::PgPool;

use crate::error::Result;

/// The `chevereto_import` row, plus the failure count.
pub struct CheveretoImportRow {
    pub last_image_id: i64,
    pub total: i64,
    pub done: i64,
    pub imported: i64,
    pub rewritten_rows: i64,
    pub failed: i64,
    /// RFC 3339, UTC.
    pub started_at: Option<String>,
    /// RFC 3339, UTC.
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

pub async fn get(pool: &PgPool) -> Result<CheveretoImportRow> {
    Ok(sqlx::query_as!(
        CheveretoImportRow,
        "select last_image_id, total, done, imported, rewritten_rows, \
                (select count(*) from chevereto_import_failures) as \"failed!\", \
                to_char(started_at at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as started_at, \
                to_char(finished_at at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as finished_at, \
                error \
         from chevereto_import"
    )
    .fetch_one(pool)
    .await?)
}

/// Lowest failed image id, None without failures.
pub async fn first_failed_image_id(pool: &PgPool) -> Result<Option<i64>> {
    Ok(
        sqlx::query_scalar!("select min(image_id) from chevereto_import_failures")
            .fetch_one(pool)
            .await?,
    )
}

/// Start a run resuming after `last_image_id`, with `total` images to process.
/// Failures after `last_image_id` are dropped, the run replays those images.
pub async fn start(pool: &PgPool, last_image_id: i64, total: i64) -> Result<()> {
    let mut transaction = pool.begin().await?;
    sqlx::query!(
        "delete from chevereto_import_failures where image_id > $1",
        last_image_id
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query!(
        "update chevereto_import set last_image_id = $1, total = $2, done = 0, imported = 0, \
         rewritten_rows = 0, started_at = now(), finished_at = null, error = null",
        last_image_id,
        total
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

/// Count one processed image.
pub async fn record_imported(pool: &PgPool) -> Result<()> {
    sqlx::query!("update chevereto_import set done = done + 1, imported = imported + 1")
        .execute(pool)
        .await?;
    Ok(())
}

/// Count one processed image that could not be imported, and list it.
pub async fn record_failure(
    pool: &PgPool,
    image_id: i64,
    fetch_url: Option<&str>,
    error: &str,
) -> Result<()> {
    let mut transaction = pool.begin().await?;
    sqlx::query!(
        "insert into chevereto_import_failures (image_id, fetch_url, error) values ($1, $2, $3) \
         on conflict (image_id) do update set fetch_url = excluded.fetch_url, error = excluded.error",
        image_id,
        fetch_url,
        error
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query!("update chevereto_import set done = done + 1")
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

/// Save a fully processed chunk: the next run resumes after `last_image_id`.
pub async fn finish_chunk(pool: &PgPool, last_image_id: i64, rewritten_rows: i64) -> Result<()> {
    sqlx::query!(
        "update chevereto_import set last_image_id = $1, rewritten_rows = rewritten_rows + $2",
        last_image_id,
        rewritten_rows
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish(pool: &PgPool, error: Option<&str>) -> Result<()> {
    sqlx::query!(
        "update chevereto_import set finished_at = now(), error = $1",
        error
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A failed image, plus the total row count (window function, as in `files::list_page`).
pub struct CheveretoImportFailureRow {
    pub image_id: i64,
    pub fetch_url: Option<String>,
    pub error: String,
    pub total: i64,
}

pub async fn failures_page(
    pool: &PgPool,
    offset: i64,
    limit: i64,
) -> Result<Vec<CheveretoImportFailureRow>> {
    Ok(sqlx::query_as!(
        CheveretoImportFailureRow,
        "select image_id, fetch_url, error, count(*) over() as \"total!\" \
         from chevereto_import_failures order by image_id offset $1 limit $2",
        offset,
        limit
    )
    .fetch_all(pool)
    .await?)
}

pub async fn failures_count(pool: &PgPool) -> Result<i64> {
    Ok(
        sqlx::query_scalar!("select count(*) as \"count!\" from chevereto_import_failures")
            .fetch_one(pool)
            .await?,
    )
}
