use std::collections::HashMap;
use std::sync::Mutex;

use sqlx::PgPool;

use crate::repository::files;

/// Image and medium views not yet written to the DB, as `(views,
/// medium_views)` per `(id, referrer)`. Serving only bumps these in memory,
/// so an image hit never touches Postgres; `flush` writes them all in one query.
// ponytail: views since the last flush are lost if the process crashes (a clean
// shutdown flushes), fine for a counter.
#[derive(Default)]
pub struct ViewCounter(Mutex<HashMap<(String, String), (i64, i64)>>);

impl ViewCounter {
    /// Count one view of an image, or of its medium, from `referrer` (the
    /// `Referer` host, empty without one).
    pub fn count(&self, id: &str, referrer: &str, medium: bool) {
        let mut pending = self.0.lock().expect("view counter lock poisoned");
        let counts = pending
            .entry((id.to_owned(), referrer.to_owned()))
            .or_default();
        if medium {
            counts.1 += 1;
        } else {
            counts.0 += 1;
        }
    }

    /// Write the pending views to the DB. On error they are logged and dropped.
    pub async fn flush(&self, pool: &PgPool) {
        let pending = std::mem::take(&mut *self.0.lock().expect("view counter lock poisoned"));
        if pending.is_empty() {
            return;
        }
        let mut ids = Vec::with_capacity(pending.len());
        let mut referrers = Vec::with_capacity(pending.len());
        let mut views = Vec::with_capacity(pending.len());
        let mut medium_views = Vec::with_capacity(pending.len());
        for ((id, referrer), (image, medium)) in pending {
            ids.push(id);
            referrers.push(referrer);
            views.push(image);
            medium_views.push(medium);
        }
        if let Err(error) = files::add_views(pool, &ids, &referrers, &views, &medium_views).await {
            tracing::warn!("failed to flush image views: {error:?}");
        }
    }
}
