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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::RwLock;

use crate::error::{Error, Result};

/// Set once the server stops, background jobs then stop at their next safe point.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
/// Read-held by every running background job, so `stop_jobs` can wait for them.
static RUNNING_JOBS: RwLock<()> = RwLock::const_new(());

/// Run a background job, registered so `stop_jobs` waits for it.
pub async fn track_job<T>(job: impl Future<Output = T>) -> T {
    let _running = RUNNING_JOBS.read().await;
    job.await
}

/// Fails once the server is stopping, for jobs to call between two safe steps.
pub fn check_shutdown() -> Result<()> {
    if SHUTTING_DOWN.load(Ordering::SeqCst) {
        return Err(Error::BadRequest("interrupted by a server shutdown".into()));
    }
    Ok(())
}

/// Ask the background jobs to stop and wait for them, at most `timeout`.
/// Returns whether they all stopped in time.
pub async fn stop_jobs(timeout: Duration) -> bool {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    tokio::time::timeout(timeout, RUNNING_JOBS.write())
        .await
        .is_ok()
}

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
        tokio::spawn(track_job(async move {
            // Run in its own task so a panic still clears `running` below.
            let result = tokio::spawn(job).await.unwrap_or(Err(Error::Internal));
            if let Err(error) = &result {
                tracing::error!("{name} failed: {error}");
            }
            self.update(|progress| {
                *progress.running() = false;
                *progress.error() = result.err().map(|error| error.to_string());
            });
        }));
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
