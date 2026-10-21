pub mod auth;
pub mod files;
pub mod fixtures;
pub mod image;
pub mod sso;
pub mod views;

use std::num::NonZeroU32;

use crate::error::Result;

/// Fetch one page (1-based) of a list whose rows carry the total row count
/// (`count(*) over()`). `fetch` gets the row offset. An out-of-range page has
/// no row to read the total from, so `count` provides it then. Returns the
/// rows and the total.
pub async fn paginate<R, Rows, Count>(
    page: NonZeroU32,
    per_page: i64,
    fetch: impl FnOnce(i64) -> Rows,
    count: Count,
    total: impl Fn(&R) -> i64,
) -> Result<(Vec<R>, i64)>
where
    Rows: Future<Output = Result<Vec<R>>>,
    Count: Future<Output = Result<i64>>,
{
    let rows = fetch((i64::from(page.get()) - 1) * per_page).await?;
    let total = match rows.first() {
        Some(row) => total(row),
        None => count.await?,
    };
    Ok((rows, total))
}
