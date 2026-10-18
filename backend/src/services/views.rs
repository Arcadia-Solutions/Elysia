use std::collections::HashMap;
use std::sync::Mutex;

use sqlx::PgPool;

use crate::repository::files;

/// Image and thumbnail views not yet written to the DB, as `(views,
/// thumbnail_views)` per id. Serving only bumps these in memory, so an image hit
/// never touches Postgres; `flush` writes them all in one query.
// ponytail: views since the last flush are lost if the process crashes (a clean
// shutdown flushes), fine for a counter.
#[derive(Default)]
pub struct ViewCounter(Mutex<HashMap<String, (i64, i64)>>);

impl ViewCounter {
    /// Count one view of an image, or of its thumbnail.
    pub fn count(&self, id: &str, thumbnail: bool) {
        let mut pending = self.0.lock().expect("view counter lock poisoned");
        let counts = match pending.get_mut(id) {
            Some(counts) => counts,
            None => pending.entry(id.to_owned()).or_default(),
        };
        if thumbnail {
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
        let mut views = Vec::with_capacity(pending.len());
        let mut thumbnail_views = Vec::with_capacity(pending.len());
        for (id, (image, thumbnail)) in pending {
            ids.push(id);
            views.push(image);
            thumbnail_views.push(thumbnail);
        }
        if let Err(error) = files::add_views(pool, &ids, &views, &thumbnail_views).await {
            tracing::warn!("failed to flush image views: {error:?}");
        }
    }
}
