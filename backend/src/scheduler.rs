//! Background tasks run on a fixed schedule, next to the HTTP server.

use std::time::Duration;

use sqlx::PgPool;

use crate::services::files;
use crate::services::views::ViewCounter;
use crate::storage::Storage;

/// Seconds between two flushes of the pending image views.
const FLUSH_INTERVAL_SECONDS: u64 = 10;

/// Seconds between two wipes of the expired trash.
const TRASH_WIPE_INTERVAL_SECONDS: u64 = 3600;

/// Run the scheduled tasks forever, on a one second tick. Spawned at startup,
/// aborted on shutdown before a last `flush`.
pub async fn handle(pool: &PgPool, storage: &Storage, views: &ViewCounter) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut seconds = 0_u64;
    loop {
        interval.tick().await;
        seconds += 1;

        if seconds.is_multiple_of(FLUSH_INTERVAL_SECONDS) {
            flush(pool, views).await;
        }
        if seconds.is_multiple_of(TRASH_WIPE_INTERVAL_SECONDS)
            && let Err(error) =
                files::wipe_trash(pool, storage, files::TRASH_RETENTION_DAYS, None).await
        {
            tracing::warn!("failed to wipe the expired trash: {error:?}");
        }
    }
}

/// Write the in-memory pending updates to the DB.
pub async fn flush(pool: &PgPool, views: &ViewCounter) {
    views.flush(pool).await;
}
