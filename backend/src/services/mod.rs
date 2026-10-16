pub mod auth;
pub mod files;
pub mod image;
pub mod sso;

use crate::error::Result;

/// Fetch one page (1-based, clamped to 1) of a list whose rows carry the total
/// row count (`count(*) over()`). `fetch` gets the row offset. An out-of-range
/// page has no row to read the total from, so `count` provides it then.
/// Returns the rows, the total and the page actually used.
pub async fn paginate<R, Rows, Count>(
    page: i64,
    per_page: i64,
    fetch: impl FnOnce(i64) -> Rows,
    count: Count,
    total: impl Fn(&R) -> i64,
) -> Result<(Vec<R>, i64, i64)>
where
    Rows: Future<Output = Result<Vec<R>>>,
    Count: Future<Output = Result<i64>>,
{
    let page = page.max(1);
    let rows = fetch((page - 1) * per_page).await?;
    let total = match rows.first() {
        Some(row) => total(row),
        None => count.await?,
    };
    Ok((rows, total, page))
}
