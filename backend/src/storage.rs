use std::path::PathBuf;

use actix_files::NamedFile;
use serde::Serialize;
use tempfile::NamedTempFile;
use utoipa::ToSchema;

use crate::error::{Error, Result};

/// Space of the filesystem holding the uploads.
#[derive(Serialize, ToSchema)]
pub struct DiskSpace {
    /// Bytes Elysia may still write.
    pub available_bytes: u64,
    pub total_bytes: u64,
    /// Uploads are refused below this many available bytes.
    pub min_free_bytes: u64,
}

/// A generated, smaller version of a stored image, served from `/<letter>/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preview {
    /// Fits within the medium box, for embeds.
    Medium,
    /// Cropped to the thumbnail box, for browse pages.
    Thumbnail,
}

impl Preview {
    pub const ALL: [Preview; 2] = [Preview::Medium, Preview::Thumbnail];

    /// Route prefix and on-disk suffix: `m` or `t`.
    pub fn letter(self) -> &'static str {
        match self {
            Preview::Medium => "m",
            Preview::Thumbnail => "t",
        }
    }
}

/// Owns the on-disk layout for uploaded files. Every path, persist, open and
/// delete goes through here so handlers/services never hand-build a path.
///
/// Single filesystem backend by design. If a second backend (S3, ...) ever
/// lands, turn this into a trait then, not before.
pub struct Storage {
    root: PathBuf,
    min_free_bytes: u64,
}

impl Storage {
    pub fn new(upload_dir: &str, min_free_bytes: u64) -> Self {
        Self {
            root: PathBuf::from(upload_dir.trim_end_matches('/')),
            min_free_bytes,
        }
    }

    pub fn disk_space(&self) -> Result<DiskSpace> {
        let stat = rustix::fs::statvfs(&self.root).map_err(std::io::Error::from)?;
        Ok(DiskSpace {
            available_bytes: stat.f_bavail * stat.f_frsize,
            total_bytes: stat.f_blocks * stat.f_frsize,
            min_free_bytes: self.min_free_bytes,
        })
    }

    /// Create the root and trash dirs if missing. Called once at startup.
    pub async fn ensure_root(&self) -> Result<()> {
        tokio::fs::create_dir_all(self.trash_dir()).await?;
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
    /// filesystem. Uploads and URL rehosts stream their bytes into one.
    /// Refused once the disk is down to `min_free_bytes`, before anything is written.
    pub fn new_temp(&self) -> Result<NamedTempFile> {
        let available_bytes = self.disk_space()?.available_bytes;
        if available_bytes < self.min_free_bytes {
            tracing::warn!(available_bytes, "disk nearly full, upload refused");
            return Err(Error::InsufficientStorage);
        }
        Ok(NamedTempFile::new_in(&self.root)?)
    }

    /// Open a stored file for streaming. NamedFile handles range requests and
    /// conditional GET (ETag/Last-Modified) for free.
    pub fn open(&self, id: &str, ext: &str) -> std::io::Result<NamedFile> {
        NamedFile::open(self.path_for(id, ext))
    }

    /// On-disk path for a file's preview: `<root>/<shard>/<id>.<letter>.webp`.
    /// Previews are always lossy WebP, so the extension is fixed.
    pub fn preview_path_for(&self, id: &str, preview: Preview) -> PathBuf {
        self.shard_dir(id)
            .join(format!("{id}.{}.webp", preview.letter()))
    }

    /// Write a generated preview to disk. Small, so the bytes are already in
    /// memory and a plain write is enough.
    pub async fn write_preview(&self, id: &str, preview: Preview, bytes: &[u8]) -> Result<()> {
        tokio::fs::write(self.preview_path_for(id, preview), bytes).await?;
        Ok(())
    }

    /// Open a stored preview for streaming.
    pub fn open_preview(&self, id: &str, preview: Preview) -> std::io::Result<NamedFile> {
        NamedFile::open(self.preview_path_for(id, preview))
    }

    /// Trash dir: `<root>/trash`. Shard dirs are 2 chars, so it never clashes.
    // ponytail: flat dir, fine for 30 days of deletions; shard it like the
    // live files if trashing ever runs into the 100k entries range.
    fn trash_dir(&self) -> PathBuf {
        self.root.join("trash")
    }

    /// Trashed path for a file: `<root>/trash/<id>.<ext>`.
    fn trash_path_for(&self, id: &str, ext: &str) -> PathBuf {
        self.trash_dir().join(format!("{id}.{ext}"))
    }

    /// Trashed path for a file's preview: `<root>/trash/<id>.<letter>.webp`.
    fn trash_preview_path_for(&self, id: &str, preview: Preview) -> PathBuf {
        self.trash_dir()
            .join(format!("{id}.{}.webp", preview.letter()))
    }

    /// Move a file and its previews out of the served tree into the trash, so
    /// `/i/`, `/m/` and `/t/` stop serving them. Missing files are skipped, so a
    /// retry after a partial failure goes through.
    pub async fn trash(&self, id: &str, ext: &str) -> Result<()> {
        ignore_missing(
            tokio::fs::rename(self.path_for(id, ext), self.trash_path_for(id, ext)).await,
        )?;
        for preview in Preview::ALL {
            ignore_missing(
                tokio::fs::rename(
                    self.preview_path_for(id, preview),
                    self.trash_preview_path_for(id, preview),
                )
                .await,
            )?;
        }
        Ok(())
    }

    /// Move a trashed file and its previews back into the served tree. A
    /// missing file is an error (wiped meanwhile), a missing preview is not.
    pub async fn restore(&self, id: &str, ext: &str) -> Result<()> {
        self.prepare_shard(id).await?;
        tokio::fs::rename(self.trash_path_for(id, ext), self.path_for(id, ext)).await?;
        for preview in Preview::ALL {
            ignore_missing(
                tokio::fs::rename(
                    self.trash_preview_path_for(id, preview),
                    self.preview_path_for(id, preview),
                )
                .await,
            )?;
        }
        Ok(())
    }

    /// Open a trashed file for streaming.
    pub fn open_trashed(&self, id: &str, ext: &str) -> std::io::Result<NamedFile> {
        NamedFile::open(self.trash_path_for(id, ext))
    }

    /// Open a trashed file's preview, or the trashed file itself when it has
    /// none (same fallback as the served preview).
    pub fn open_trashed_preview(
        &self,
        id: &str,
        ext: &str,
        preview: Preview,
    ) -> std::io::Result<NamedFile> {
        match NamedFile::open(self.trash_preview_path_for(id, preview)) {
            Ok(file) => Ok(file),
            Err(_) => NamedFile::open(self.trash_path_for(id, ext)),
        }
    }

    /// Delete a served file and its previews. Missing files are skipped.
    pub async fn delete(&self, id: &str, ext: &str) -> Result<()> {
        ignore_missing(tokio::fs::remove_file(self.path_for(id, ext)).await)?;
        for preview in Preview::ALL {
            ignore_missing(tokio::fs::remove_file(self.preview_path_for(id, preview)).await)?;
        }
        Ok(())
    }

    /// Delete a trashed file and its previews. Missing files are skipped.
    pub async fn wipe_trashed(&self, id: &str, ext: &str) -> Result<()> {
        ignore_missing(tokio::fs::remove_file(self.trash_path_for(id, ext)).await)?;
        for preview in Preview::ALL {
            ignore_missing(tokio::fs::remove_file(self.trash_preview_path_for(id, preview)).await)?;
        }
        Ok(())
    }
}

fn ignore_missing(result: std::io::Result<()>) -> Result<()> {
    match result {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}
