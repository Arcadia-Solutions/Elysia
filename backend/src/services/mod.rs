pub mod albums;
pub mod auth;
pub mod chevereto_import;
pub mod files;
pub mod fixtures;
pub mod image;
pub mod maintenance;
pub mod sso;
pub mod views;

use std::num::NonZeroU32;

use crate::error::Result;

/// Fetch one page (1-based) of a list whose rows carry the total row count
/// (`count(*) over()`). `fetch` gets the row offset and limit. An out-of-range page has
/// no row to read the total from, so `count` provides it then. Returns the
/// rows and the total.
pub async fn paginate<R, Rows, Count>(
    page: NonZeroU32,
    page_size: u32,
    fetch: impl FnOnce(i64, i64) -> Rows,
    count: Count,
    total: impl Fn(&R) -> i64,
) -> Result<(Vec<R>, i64)>
where
    Rows: Future<Output = Result<Vec<R>>>,
    Count: Future<Output = Result<i64>>,
{
    let page_size = i64::from(page_size);
    let rows = fetch((i64::from(page.get()) - 1) * page_size, page_size).await?;
    let total = match rows.first() {
        Some(row) => total(row),
        None => count.await?,
    };
    Ok((rows, total))
}
