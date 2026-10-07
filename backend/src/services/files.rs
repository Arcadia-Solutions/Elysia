use actix_multipart::form::tempfile::TempFile;
use sqlx::PgPool;
use tokio::io::AsyncReadExt;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::repository::files::{self, FileRow, NewFile};
use crate::storage::Storage;

const ALPHABET: [char; 62] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l',
    'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9',
];

pub fn generate_id() -> String {
    nanoid::nanoid!(8, &ALPHABET)
}

/// Outcome of a stored upload; the handler shapes the HTTP response from this.
pub struct StoredFile {
    pub id: String,
    pub ext: String,
}

/// Validate, inspect and store an uploaded file: size check, type sniff,
/// dimensions, a collision-safe row insert, then persist to disk.
///
/// This is the seam where future processing (compression, conversion,
/// thumbnails, EXIF stripping, deduplication) will hang off, keeping the
/// handler a thin HTTP shell.
pub async fn store_upload(
    pool: &PgPool,
    storage: &Storage,
    config: &Config,
    file: TempFile,
) -> Result<StoredFile> {
    let size = file.size as u64;
    check_size(config, size)?;

    let path = file.file.path();

    // Sniff the type from the file header only, no need to read the whole file.
    let mut header = [0u8; 512];
    let read = {
        let mut handle = tokio::fs::File::open(path).await?;
        handle.read(&mut header).await?
    };
    let kind = infer::get(&header[..read]).ok_or(Error::UnsupportedMediaType)?;
    let ext = match kind.mime_type() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => return Err(Error::UnsupportedMediaType),
    };
    let mime = kind.mime_type().to_string();

    // Pixel dimensions, read from the image header (no full decode).
    let dimensions = imagesize::size(path).map_err(|_| Error::UnsupportedMediaType)?;
    check_dimensions(config, dimensions.width as u32, dimensions.height as u32)?;
    let (width, height) = (dimensions.width as i32, dimensions.height as i32);

    let original_name = file.file_name.clone();
    let size = size as i64;

    // Insert first (retrying on id collision) so we never leave an orphan file on disk.
    let mut id = None;
    for _ in 0..5 {
        let candidate = generate_id();
        let inserted = files::insert(
            pool,
            &NewFile {
                id: &candidate,
                ext,
                mime: &mime,
                original_name: original_name.as_deref(),
                size,
                width,
                height,
            },
        )
        .await?;
        if inserted {
            id = Some(candidate);
            break;
        }
    }
    let id = id.ok_or_else(|| Error::BadRequest("could not allocate an id".into()))?;

    storage.persist(file, &id, ext)?;

    Ok(StoredFile {
        id,
        ext: ext.to_string(),
    })
}

/// Reject files above the configured byte limit (0 = unlimited).
pub fn check_size(config: &Config, size: u64) -> Result<()> {
    let limit = config.storage.max_file_size_bytes;
    if limit > 0 && size > limit {
        return Err(Error::BadRequest(format!(
            "file too large: {size} bytes, limit is {limit} bytes"
        )));
    }
    Ok(())
}

/// Reject images above the configured pixel limits (0 = unlimited, per axis).
pub fn check_dimensions(config: &Config, width: u32, height: u32) -> Result<()> {
    let max_width = config.storage.max_width_pixels;
    let max_height = config.storage.max_height_pixels;
    if max_width > 0 && width > max_width {
        return Err(Error::BadRequest(format!(
            "image width too large: {width} pixels, limit is {max_width} pixels"
        )));
    }
    if max_height > 0 && height > max_height {
        return Err(Error::BadRequest(format!(
            "image height too large: {height} pixels, limit is {max_height} pixels"
        )));
    }
    Ok(())
}

/// Look up a file's metadata by id, or `NotFound`. The row doubles as a
/// path-traversal guard: only ids that exist are ever served.
pub async fn get(pool: &PgPool, id: &str) -> Result<FileRow> {
    files::find(pool, id).await?.ok_or(Error::NotFound)
}
