pub mod albums;
pub mod auth;
pub mod chevereto_import;
pub mod files;
pub mod fixtures;
pub mod image;
pub mod maintenance;
pub mod similar_images;
pub mod sso;
pub mod views;

use std::num::NonZeroU32;
use std::sync::Mutex;

use crate::error::{Error, Result};

/// Progress of a `Job`, read by its progress endpoint.
pub trait JobProgress: Clone + Send + 'static {
    fn running(&mut self) -> &mut bool;
    /// Why the last run stopped early, if it did.
    fn error(&mut self) -> &mut Option<String>;
}

/// A background job running one at a time per process, with its progress.
// ponytail: progress in memory, reset on restart (results belong in the database).
pub struct Job<P>(Mutex<P>);

impl<P: JobProgress> Job<P> {
    pub const fn new(progress: P) -> Self {
        Self(Mutex::new(progress))
    }

    pub fn update(&self, change: impl FnOnce(&mut P)) {
        change(&mut self.0.lock().expect("job lock poisoned"));
    }

    pub fn progress(&self) -> P {
        self.0.lock().expect("job lock poisoned").clone()
    }

    /// Reset the progress with `reset` and run `job` in the background,
    /// logging its failure as `name`'s. Fails if a run is already going.
    pub fn spawn(
        &'static self,
        name: impl std::fmt::Display + Send + 'static,
        reset: impl FnOnce(&mut P),
        job: impl Future<Output = Result<()>> + Send + 'static,
    ) -> Result<()> {
        {
            let mut progress = self.0.lock().expect("job lock poisoned");
            if *progress.running() {
                return Err(Error::Conflict);
            }
            reset(&mut progress);
            *progress.running() = true;
            *progress.error() = None;
        }
        tokio::spawn(async move {
            // Run in its own task so a panic still clears `running` below.
            let result = tokio::spawn(job).await.unwrap_or(Err(Error::Internal));
            if let Err(error) = &result {
                tracing::error!("{name} failed: {error}");
            }
            self.update(|progress| {
                *progress.running() = false;
                *progress.error() = result.err().map(|error| error.to_string());
            });
        });
        Ok(())
    }
}

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
