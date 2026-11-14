//! Checksum columns of `files`, one per `checksum_algorithm` label. They are
//! read through `to_jsonb(files)` and written and looked up by SQL functions,
//! so queries never name a column from a value.

use sqlx::PgPool;

use crate::error::Result;
use crate::repository::files::{FileRecord, OnDiskRow};
use crate::services::checksums::ChecksumAlgorithm;

/// One computed checksum of a file's stored bytes.
pub struct ChecksumRow {
    pub algorithm: ChecksumAlgorithm,
    pub digest: String,
}

/// Set checksums of a file, the digest at the same index as its algorithm.
pub async fn set(
    pool: &PgPool,
    file_id: &str,
    algorithms: &[ChecksumAlgorithm],
    digests: &[String],
) -> Result<()> {
    sqlx::query!(
        "select set_file_checksums($1, $2, $3)",
        file_id,
        algorithms as &[ChecksumAlgorithm],
        digests,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A file's computed checksums, in algorithm order.
pub async fn for_file(pool: &PgPool, file_id: &str) -> Result<Vec<ChecksumRow>> {
    Ok(sqlx::query_as!(
        ChecksumRow,
        r#"select checksum.key::checksum_algorithm as "algorithm!: ChecksumAlgorithm",
                  checksum.value as "digest!"
           from files, jsonb_each_text(to_jsonb(files)) as checksum
           where id = $1 and checksum.value is not null
             and checksum.key = any(enum_range(null::checksum_algorithm)::text[])
           order by 1"#,
        file_id,
    )
    .fetch_all(pool)
    .await?)
}

/// Available file whose stored bytes have this checksum.
pub async fn find_record(
    pool: &PgPool,
    algorithm: ChecksumAlgorithm,
    digest: &str,
) -> Result<Option<FileRecord>> {
    Ok(sqlx::query_as!(
        FileRecord,
        r#"select id, ext, mime, original_ext, original_width, original_height,
                width, height, applied_quality, medium_size > 0 as "has_medium!", thumbnail_size > 0 as "has_thumbnail!"
         from files where id = file_id_by_checksum($1, $2)"#,
        algorithm as ChecksumAlgorithm,
        digest,
    )
    .fetch_optional(pool)
    .await?)
}

/// How many available files have each computed algorithm.
pub async fn counts(pool: &PgPool) -> Result<Vec<(ChecksumAlgorithm, i64)>> {
    Ok(sqlx::query!(
        r#"select checksum.key::checksum_algorithm as "algorithm!: ChecksumAlgorithm",
                  count(*) as "count!"
           from files, jsonb_each_text(to_jsonb(files)) as checksum
           where status = 'available' and checksum.value is not null
             and checksum.key = any(enum_range(null::checksum_algorithm)::text[])
           group by 1 order by 1"#,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.algorithm, row.count))
    .collect())
}

/// How many available files miss one of `algorithms`.
pub async fn count_missing(pool: &PgPool, algorithms: &[ChecksumAlgorithm]) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"select count(*) as "count!" from files
           where status = 'available'
             and exists (select 1 from unnest($1::checksum_algorithm[]) as algorithm
                         where to_jsonb(files) ->> algorithm::text is null)"#,
        algorithms as &[ChecksumAlgorithm],
    )
    .fetch_one(pool)
    .await?)
}

/// Up to `limit` available files with an id above `after` missing one of
/// `algorithms`, by id.
pub async fn list_missing(
    pool: &PgPool,
    algorithms: &[ChecksumAlgorithm],
    after: &str,
    limit: i64,
) -> Result<Vec<OnDiskRow>> {
    Ok(sqlx::query_as!(
        OnDiskRow,
        r#"select id, ext from files
           where status = 'available' and id > $2
             and exists (select 1 from unnest($1::checksum_algorithm[]) as algorithm
                         where to_jsonb(files) ->> algorithm::text is null)
           order by id limit $3"#,
        algorithms as &[ChecksumAlgorithm],
        after,
        limit,
    )
    .fetch_all(pool)
    .await?)
}
