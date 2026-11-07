use std::collections::HashMap;
use std::sync::Mutex;

use sqlx::PgPool;

use crate::repository::files;
use crate::storage::Preview;

/// Views of an image and of its previews.
#[derive(Default)]
struct Views {
    image: i64,
    medium: i64,
    thumbnail: i64,
}

/// Views not yet written to the DB, per `(id, referrer)`. Serving only bumps
/// these in memory, so an image hit never touches Postgres; `flush` writes them
/// all in one query.
// ponytail: views since the last flush are lost if the process crashes (a clean
// shutdown flushes), fine for a counter.
#[derive(Default)]
pub struct ViewCounter(Mutex<HashMap<(String, String), Views>>);

impl ViewCounter {
    /// Count one view of an image (`None`), or of one of its previews, from
    /// `referrer` (the `Referer` host, empty without one).
    pub fn count(&self, id: &str, referrer: &str, preview: Option<Preview>) {
        let mut pending = self.0.lock().expect("view counter lock poisoned");
        let views = pending
            .entry((id.to_owned(), referrer.to_owned()))
            .or_default();
        match preview {
            None => views.image += 1,
            Some(Preview::Medium) => views.medium += 1,
            Some(Preview::Thumbnail) => views.thumbnail += 1,
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
        let mut image_views = Vec::with_capacity(pending.len());
        let mut medium_views = Vec::with_capacity(pending.len());
        let mut thumbnail_views = Vec::with_capacity(pending.len());
        for ((id, referrer), views) in pending {
            ids.push(id);
            referrers.push(referrer);
            image_views.push(views.image);
            medium_views.push(views.medium);
            thumbnail_views.push(views.thumbnail);
        }
        if let Err(error) = files::add_views(
            pool,
            &ids,
            &referrers,
            &image_views,
            &medium_views,
            &thumbnail_views,
        )
        .await
        {
            tracing::warn!("failed to flush image views: {error:?}");
        }
    }
}
