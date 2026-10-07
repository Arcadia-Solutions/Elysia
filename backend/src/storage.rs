use std::path::PathBuf;

use actix_files::NamedFile;
use actix_multipart::form::tempfile::TempFile;

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

    /// On-disk path for a file: `<root>/<id>.<ext>`.
    pub fn path_for(&self, id: &str, ext: &str) -> PathBuf {
        self.root.join(format!("{id}.{ext}"))
    }

    /// Move a finished upload into place. `file` sits in a temp file inside
    /// `root` already (TempFileConfig in main.rs), so this is a same-filesystem
    /// rename: atomic, no re-copy of the bytes.
    pub fn persist(&self, file: TempFile, id: &str, ext: &str) -> Result<()> {
        file.file
            .persist(self.path_for(id, ext))
            .map_err(|e| e.error)?;
        Ok(())
    }

    /// Open a stored file for streaming. NamedFile handles range requests and
    /// conditional GET (ETag/Last-Modified) for free.
    pub async fn open(&self, id: &str, ext: &str) -> std::io::Result<NamedFile> {
        NamedFile::open_async(self.path_for(id, ext)).await
    }
}
