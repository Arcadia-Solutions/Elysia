//! Finding images that look alike by their perceptual hash.
//!
//! Images are hashed on upload. A scan runs in the background: it groups the
//! available images whose hashes differ in at most `max_distance` bits and
//! stores the groups in `similar_files`.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::{Error, Result};
use crate::repository::files::{self, ImageListRow};
use crate::services::{Job, JobProgress, check_shutdown, paginate};

/// Largest `max_distance` accepted: unrelated images differ in about half of
/// the 63 used hash bits, so past a quarter they start matching and transitive
/// grouping merges them all. Also in `SimilarScanParams`' schema, which takes literals only.
pub const MAX_DISTANCE: u32 = 16;

#[derive(Clone, Copy, Deserialize, ToSchema)]
pub struct SimilarScanParams {
    /// Most bits two perceptual hashes (64 bits) may differ in for their images
    /// to be grouped: 0 only matches visually identical images, around 10 also
    /// matches recompressed or slightly edited copies.
    #[schema(maximum = 16)]
    pub max_distance: u32,
}

/// State of the current or last scan.
#[derive(Clone, Serialize, ToSchema)]
pub struct SimilarProgress {
    pub running: bool,
    /// Images compared with all the others so far.
    pub done: usize,
    /// Images to compare.
    pub total: usize,
    /// Why the last scan stopped early, if it did.
    pub error: Option<String>,
    /// `max_distance` of the current or last scan, a default before the first.
    pub max_distance: u32,
    /// When the last successful scan finished, RFC 3339, UTC. None before the
    /// first since the server started.
    pub scanned_at: Option<String>,
}

impl JobProgress for SimilarProgress {
    fn running(&mut self) -> &mut bool {
        &mut self.running
    }

    fn error(&mut self) -> &mut Option<String> {
        &mut self.error
    }
}

static JOB: Job<SimilarProgress> = Job::new(SimilarProgress {
    running: false,
    done: 0,
    total: 0,
    error: None,
    max_distance: 8,
    scanned_at: None,
});

pub fn progress() -> SimilarProgress {
    JOB.progress()
}

/// Start scanning for similar images in the background. Fails if a scan is
/// already going or `params.max_distance` is above `MAX_DISTANCE`.
pub fn start_scan(pool: PgPool, params: SimilarScanParams) -> Result<()> {
    if params.max_distance > MAX_DISTANCE {
        return Err(Error::BadRequest(format!(
            "max_distance must be at most {MAX_DISTANCE}"
        )));
    }
    JOB.spawn(
        "similar images scan",
        |progress| {
            progress.done = 0;
            progress.total = 0;
            progress.max_distance = params.max_distance;
        },
        async move { scan(&pool, params.max_distance).await },
    )
}

async fn scan(pool: &PgPool, max_distance: u32) -> Result<()> {
    let rows = files::phashes(pool).await?;
    JOB.update(|progress| progress.total = rows.len());
    let hashes: Vec<i64> = rows.iter().map(|row| row.phash).collect();
    let groups = tokio::task::spawn_blocking(move || {
        group_similar(&hashes, max_distance, |done| {
            JOB.update(|progress| progress.done = done);
            check_shutdown()
        })
    })
    .await
    .map_err(|_| Error::Internal)??;

    let mut ids = Vec::new();
    let mut group_ids = Vec::new();
    for (group_id, members) in groups.iter().enumerate() {
        for &member in members {
            ids.push(rows[member].id.clone());
            group_ids.push(group_id as i32);
        }
    }
    let scanned_at = files::replace_similar(pool, &ids, &group_ids).await?;
    JOB.update(|progress| progress.scanned_at = Some(scanned_at));
    Ok(())
}

/// Group the indexes of `hashes` differing in at most `max_distance` bits,
/// transitively: groups of two or more, each in index order, by first index.
/// `on_progress` gets how many hashes were compared with all later ones, its
/// error stops the grouping.
// ponytail: compares every pair, O(n²): seconds for 100k images, minutes past
// a million; index the hashes in a BK-tree if that gets too slow.
pub fn group_similar(
    hashes: &[i64],
    max_distance: u32,
    on_progress: impl Fn(usize) -> Result<()>,
) -> Result<Vec<Vec<usize>>> {
    // Union-find: each index points towards its group's root.
    let mut parents: Vec<usize> = (0..hashes.len()).collect();
    fn root(parents: &mut [usize], mut index: usize) -> usize {
        while parents[index] != index {
            parents[index] = parents[parents[index]];
            index = parents[index];
        }
        index
    }
    for (index, hash) in hashes.iter().enumerate() {
        for (other, other_hash) in hashes.iter().enumerate().skip(index + 1) {
            if (hash ^ other_hash).count_ones() <= max_distance {
                let (first_root, other_root) =
                    (root(&mut parents, index), root(&mut parents, other));
                parents[other_root.max(first_root)] = other_root.min(first_root);
            }
        }
        if index % 1000 == 999 {
            on_progress(index + 1)?;
        }
    }
    on_progress(hashes.len())?;
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); hashes.len()];
    for index in 0..hashes.len() {
        let group_root = root(&mut parents, index);
        groups[group_root].push(index);
    }
    groups.retain(|members| members.len() > 1);
    Ok(groups)
}

/// One page (1-based) of the last scan's groups still holding two available
/// images, each with its images, largest first, with the number of groups.
pub async fn list_similar(
    pool: &PgPool,
    page: NonZeroU32,
    page_size: u32,
) -> Result<(Vec<Vec<ImageListRow>>, i64)> {
    let (rows, total) = paginate(
        page,
        page_size,
        |offset, limit| files::similar_page(pool, offset, limit),
        files::similar_group_count(pool),
        |(_, row)| row.total,
    )
    .await?;
    let mut groups: Vec<Vec<ImageListRow>> = Vec::new();
    let mut last_group_id = None;
    for (group_id, row) in rows {
        match groups.last_mut() {
            Some(group) if last_group_id == Some(group_id) => group.push(row),
            _ => groups.push(vec![row]),
        }
        last_group_id = Some(group_id);
    }
    Ok((groups, total))
}
