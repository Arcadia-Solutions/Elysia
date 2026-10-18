use std::path::PathBuf;

use actix_files::NamedFile;
use tempfile::NamedTempFile;

use crate::error::Result;

/// Owns the on-disk layout for uploaded files. Every path, persist, open and
/// delete goes through here so handlers/services never hand-build a path.
///
/// Single filesystem backend by design. If a second backend (S3, ...) ever
/// lands, turn this into a trait then, not before.
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(upload_dir: &str) -> Self {
        Self {
            root: PathBuf::from(upload_dir.trim_end_matches('/')),
        }
    }

    /// Create the root dir if missing. Called once at startup.
    pub async fn ensure_root(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    /// Shard dir for an id: `<root>/<first 2 id chars>`. Ids are always 8 chars.
    // ponytail: one level of 3844 shards, ~190M images at 100k entries per dir;
    // inodes or disk size run out first, revisit only past that.
    fn shard_dir(&self, id: &str) -> PathBuf {
        self.root.join(&id[..2])
    }

    /// Create the shard dir for an id if missing. Call before writing its files.
    pub async fn prepare_shard(&self, id: &str) -> Result<()> {
        tokio::fs::create_dir_all(self.shard_dir(id)).await?;
        Ok(())
    }

    /// On-disk path for a file: `<root>/<shard>/<id>.<ext>`.
    pub fn path_for(&self, id: &str, ext: &str) -> PathBuf {
        self.shard_dir(id).join(format!("{id}.{ext}"))
    }

    /// A temp file inside `root`, so a later `persist` stays on the same
    /// filesystem. Used for URL rehosts, where the bytes arrive over HTTP rather
    /// than through a multipart `TempFile`.
    pub fn new_temp(&self) -> Result<NamedTempFile> {
        Ok(NamedTempFile::new_in(&self.root)?)
    }

    /// Open a stored file for streaming. NamedFile handles range requests and
    /// conditional GET (ETag/Last-Modified) for free.
    pub async fn open(&self, id: &str, ext: &str) -> std::io::Result<NamedFile> {
        NamedFile::open_async(self.path_for(id, ext)).await
    }

    /// On-disk path for a file's thumbnail: `<root>/<shard>/<id>.t.webp`.
    /// Thumbnails are always lossy WebP, so the extension is fixed.
    pub fn thumbnail_path_for(&self, id: &str) -> PathBuf {
        self.shard_dir(id).join(format!("{id}.t.webp"))
    }

    /// Write a generated thumbnail to disk. Small (a few KiB), so the bytes are
    /// already in memory and a plain write is enough.
    pub async fn write_thumbnail(&self, id: &str, bytes: &[u8]) -> Result<()> {
        tokio::fs::write(self.thumbnail_path_for(id), bytes).await?;
        Ok(())
    }

    /// Open a stored thumbnail for streaming.
    pub async fn open_thumbnail(&self, id: &str) -> std::io::Result<NamedFile> {
        NamedFile::open_async(self.thumbnail_path_for(id)).await
    }
}
