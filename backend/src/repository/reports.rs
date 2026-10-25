use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::Result;
use crate::repository::files::ImageListRow;

/// Report an available file. False when there is no such available file.
pub async fn create(pool: &PgPool, file_id: &str, reporter_id: Uuid, reason: &str) -> Result<bool> {
    let res = sqlx::query!(
        r#"insert into reports (file_id, reporter_id, reason)
         select id, $2, $3 from files where id = $1 and status = 'available'"#,
        file_id,
        reporter_id,
        reason
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Delete every report on these files.
pub async fn dismiss(pool: &PgPool, file_ids: &[String]) -> Result<()> {
    sqlx::query!(r#"delete from reports where file_id = any($1)"#, file_ids)
        .execute(pool)
        .await?;
    Ok(())
}

/// One user's report on an image.
#[derive(Serialize, ToSchema)]
pub struct Report {
    pub reporter_username: String,
    pub reason: String,
    /// RFC 3339, UTC.
    pub created_at: String,
}

/// A page of reported available files, the most reported first, then the one
/// with the oldest report; one row per report, grouped by file, oldest first.
/// `total` counts files.
pub async fn queue_page(
    pool: &PgPool,
    offset: i64,
    limit: i64,
) -> Result<Vec<(ImageListRow, Report)>> {
    let rows = sqlx::query!(
        r#"with reported as (
           select file_id, count(*) as report_count, min(reports.created_at) as first_reported_at
           from reports join files on files.id = file_id
           where files.status = 'available' group by file_id
         ), page as (
           select file_id, report_count, first_reported_at, count(*) over() as total
           from reported order by report_count desc, first_reported_at, file_id
           offset $1 limit $2
         )
         select page.total as "total!",
                f.id, f.ext, f.original_name, f.size, f.width, f.height, f.has_thumbnail,
                f.views, uploader.username as uploader_username,
                to_char(f.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "created_at!",
                reporter.username as reporter_username, r.reason,
                to_char(r.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                  as "reported_at!"
         from page
         join files f on f.id = page.file_id
         join reports r on r.file_id = page.file_id
         join users uploader on uploader.id = f.uploaded_by_user_id
         join users reporter on reporter.id = r.reporter_id
         order by page.report_count desc, page.first_reported_at, f.id, r.created_at, r.id"#,
        offset,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
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
                Report {
                    reporter_username: row.reporter_username,
                    reason: row.reason,
                    created_at: row.reported_at,
                },
            )
        })
        .collect())
}

/// Number of reported available files, for an out-of-range queue page.
pub async fn queue_count(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"select count(distinct file_id) as "count!"
         from reports join files on files.id = file_id where files.status = 'available'"#
    )
    .fetch_one(pool)
    .await?)
}
