use std::io::Write;

use actix_multipart::form::tempfile::TempFile;
use serde::Serialize;
use sqlx::PgPool;
use tempfile::NamedTempFile;
use tokio::io::AsyncReadExt;

use crate::error::{Error, Result};
use crate::repository::files::{self, FileRow, NewFile};
use crate::services::image::{self, TargetFormat};
use crate::settings::ElysiaSettings;
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
    /// True when the bytes were already stored and this id is the existing one
    /// (deduplicated), false when a new file was created.
    pub existed: bool,
    /// None when the stored file is identical to its source.
    pub actions: Option<Actions>,
    /// True when a generated WebP thumbnail exists; when false `/t/` serves the
    /// original file, so the thumbnail URL keeps the original extension.
    pub has_thumbnail: bool,
}

impl StoredFile {
    /// Public URL for the stored image.
    pub fn image_url(&self) -> String {
        format!("/i/{}.{}", self.id, self.ext)
    }

    /// Public URL for the thumbnail. Generated thumbnails are always WebP;
    /// without one, `/t/` falls back to the original, hence its extension.
    pub fn thumbnail_url(&self) -> String {
        let extension = if self.has_thumbnail {
            "webp"
        } else {
            &self.ext
        };
        format!("/t/{}.{}", self.id, extension)
    }
}

/// Per-upload processing options.
#[derive(Default)]
pub struct UploadOptions {
    pub lossy_compression_value: Option<u8>,
    /// Effective output format: the per-upload override, or the configured
    /// default. `None` stores the upload as-is (processing off).
    pub target_file_format: Option<TargetFormat>,
    /// Effective EXIF-stripping choice: the per-upload override, or the
    /// configured default.
    pub strip_exif: bool,
}

/// The bytes to store plus their metadata: either the pipeline's output or, when
/// processing is off/passed through, the untouched source.
struct Output {
    temp: NamedTempFile,
    ext: &'static str,
    mime: String,
    width: i32,
    height: i32,
    size: i64,
    applied_quality: Option<i32>,
}

/// What processing changed between the source and the stored file.
#[derive(Serialize, utoipa::ToSchema)]
pub struct Actions {
    pub resize: Option<ResizeAction>,
    pub convert: Option<ConvertAction>,
    pub compression: CompressionAction,
}

/// Dimensions before and after, as `[width, height]`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ResizeAction {
    pub from: [i32; 2],
    pub to: [i32; 2],
}

/// File extensions before and after.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ConvertAction {
    pub from: String,
    pub to: String,
}

/// `"lossless"` or the applied lossy quality.
#[derive(Serialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum CompressionAction {
    Lossless(String),
    Lossy { lossy_quality: i32 },
}

/// Describe a source-to-stored difference; None when nothing changed.
fn describe_actions(
    source_ext: &str,
    stored_ext: &str,
    source_size: (i32, i32),
    stored_size: (i32, i32),
    applied_quality: Option<i32>,
) -> Option<Actions> {
    let resized = source_size != stored_size;
    let converted = source_ext != stored_ext;
    if !resized && !converted && applied_quality.is_none() {
        return None;
    }
    Some(Actions {
        resize: resized.then_some(ResizeAction {
            from: [source_size.0, source_size.1],
            to: [stored_size.0, stored_size.1],
        }),
        convert: converted.then(|| ConvertAction {
            from: source_ext.to_string(),
            to: stored_ext.to_string(),
        }),
        compression: match applied_quality {
            Some(lossy_quality) => CompressionAction::Lossy { lossy_quality },
            None => CompressionAction::Lossless("lossless".into()),
        },
    })
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
    settings: &ElysiaSettings,
    file: TempFile,
    options: &UploadOptions,
) -> Result<StoredFile> {
    store(
        pool,
        storage,
        settings,
        file.file,
        file.size as u64,
        file.file_name,
        options,
    )
    .await
}

/// Fetch an image from `url` and store it, sharing the multipart upload's
/// validation. Returns a `BadRequest` when the URL cannot be fetched, and the
/// usual `UnsupportedMediaType` when the fetched bytes are not a supported image.
pub async fn store_from_url(
    pool: &PgPool,
    storage: &Storage,
    settings: &ElysiaSettings,
    url: &str,
    options: &UploadOptions,
) -> Result<StoredFile> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| Error::BadRequest(format!("could not fetch url: {e}")))?;
    if !response.status().is_success() {
        return Err(Error::BadRequest(format!(
            "url returned status {}",
            response.status().as_u16()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| Error::BadRequest(format!("could not read url body: {e}")))?;
    check_size(settings, bytes.len() as u64)?;

    let original_name = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut temp = storage.new_temp()?;
    temp.write_all(&bytes)?;

    store(
        pool,
        storage,
        settings,
        temp,
        bytes.len() as u64,
        original_name,
        options,
    )
    .await
}

/// Shared core: validate the temp file, sniff its type, check dimensions,
/// insert a collision-safe row, then persist it to disk.
async fn store(
    pool: &PgPool,
    storage: &Storage,
    settings: &ElysiaSettings,
    temp: NamedTempFile,
    size: u64,
    original_name: Option<String>,
    options: &UploadOptions,
) -> Result<StoredFile> {
    check_size(settings, size)?;

    let path = temp.path();

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
    check_dimensions(settings, dimensions.width as u32, dimensions.height as u32)?;
    let source_ext = ext;
    let (source_width, source_height) = (dimensions.width as i32, dimensions.height as i32);
    let original_hash = hash_file(path).await?;
    let requested_quality = options.lossy_compression_value.map(|q| q as i32);

    // Same source under the same requested quality was already processed.
    // ponytail: key is (original_hash, requested_quality) only, ignoring the target
    // format/size/dimensions config. If an operator changes that config, a re-upload
    // of the same source returns the file processed under the old config.
    if let Some(record) =
        files::find_record_by_source(pool, &original_hash, requested_quality).await?
    {
        return Ok(existing_stored(record));
    }

    // Run the pipeline when a target format is in effect; `None` passes through.
    // The per-upload override wins, else the configured default.
    let target_file_format = options
        .target_file_format
        .or(settings.default_target_file_format);
    let thumbnails_enabled = settings.thumbnails_enabled();
    let thumbnail_box = (
        settings.thumbnail_width_pixels,
        settings.thumbnail_height_pixels,
    );

    // The processed main output (`None` = passthrough or animated GIF) and, when
    // the target-format pipeline runs, its thumbnail decoded from the same pixels.
    let (processed, mut thumbnail) = match target_file_format {
        Some(format) => {
            let input = tokio::fs::read(path).await?;
            let process_options = image::ProcessOptions {
                target_width: settings.target_width_pixels,
                target_height: settings.target_height_pixels,
                format,
                target_file_size: settings.target_file_size_bytes,
                requested_quality: options.lossy_compression_value,
            };
            let thumbnail_options = thumbnails_enabled.then(|| thumbnail_process_options(settings));
            // Encoding is CPU-heavy; keep it off the async worker threads. Decode
            // once and reuse the pixels for the thumbnail when one is needed.
            tokio::task::spawn_blocking(move || {
                let Some(image) = image::decode(&input)? else {
                    return Ok::<_, image::ProcessError>((None, None));
                };
                let source = thumbnail_options.is_some().then(|| image.clone());
                let processed = image::process_decoded(image, &process_options)?;
                let thumbnail = match (thumbnail_options, source) {
                    (Some(options), Some(source))
                        if over_thumbnail_box(thumbnail_box, processed.width, processed.height) =>
                    {
                        Some(image::process_decoded(source, &options)?.bytes)
                    }
                    _ => None,
                };
                Ok((Some(processed), thumbnail))
            })
            .await
            .map_err(|_| Error::Io(std::io::Error::other("image processing task failed")))?
            .map_err(process_error)?
        }
        None => (None, None),
    };

    // Passthrough stores the source, so the output matches the source dimensions;
    // read its bytes for a thumbnail only when one is actually needed, and before
    // `temp` may be consumed below.
    let passthrough_thumbnail_source = if thumbnails_enabled
        && target_file_format.is_none()
        && over_thumbnail_box(thumbnail_box, source_width as u32, source_height as u32)
    {
        Some(tokio::fs::read(path).await?)
    } else {
        None
    };

    let output = match processed {
        Some(p) => {
            let mut temp = storage.new_temp()?;
            temp.write_all(&p.bytes)?;
            Output {
                size: p.bytes.len() as i64,
                ext: p.ext,
                mime: p.mime.to_string(),
                width: p.width as i32,
                height: p.height as i32,
                applied_quality: p.applied_quality.map(|q| q as i32),
                temp,
            }
        }
        // Passthrough: store the source bytes. The conversion branch above already
        // drops metadata (re-encode starts from a bare pixel buffer), so EXIF is
        // only stripped here, when requested, by rewriting the container.
        None if options.strip_exif => {
            let input = tokio::fs::read(path).await?;
            let stripped = strip_exif(input)?;
            let mut temp = storage.new_temp()?;
            temp.write_all(&stripped)?;
            Output {
                size: stripped.len() as i64,
                ext: source_ext,
                mime,
                width: source_width,
                height: source_height,
                applied_quality: None,
                temp,
            }
        }
        None => Output {
            size: size as i64,
            ext: source_ext,
            mime,
            width: source_width,
            height: source_height,
            applied_quality: None,
            temp,
        },
    };
    // Passthrough: the source was not decoded above, so make the thumbnail from
    // its own decode. The pipeline path already produced it from the shared decode.
    if let Some(source) = passthrough_thumbnail_source {
        thumbnail = generate_thumbnail(source, settings).await?;
    }

    let hash = hash_file(output.temp.path()).await?;

    // A different source that produced identical output bytes.
    if let Some(record) = files::find_record_by_hash(pool, &hash).await? {
        return Ok(existing_stored(record));
    }

    let original_name = original_name.as_deref();

    // Insert first (retrying on id collision) so we never leave an orphan file on disk.
    let mut id = None;
    for _ in 0..5 {
        let candidate = generate_id();
        let inserted = files::insert(
            pool,
            &NewFile {
                id: &candidate,
                ext: output.ext,
                mime: &output.mime,
                original_name,
                size: output.size,
                width: output.width,
                height: output.height,
                hash: &hash,
                original_hash: &original_hash,
                original_ext: source_ext,
                original_width: source_width,
                original_height: source_height,
                requested_quality,
                applied_quality: output.applied_quality,
                has_thumbnail: thumbnail.is_some(),
            },
        )
        .await?;
        if inserted {
            id = Some(candidate);
            break;
        }
        // A failed insert is an id collision or a concurrent upload of the same
        // bytes winning the hash race; in the latter case, reuse its row.
        if let Some(record) = files::find_record_by_hash(pool, &hash).await? {
            return Ok(existing_stored(record));
        }
    }
    let id = id.ok_or_else(|| Error::BadRequest("could not allocate an id".into()))?;

    storage.persist(output.temp, &id, output.ext)?;
    if let Some(bytes) = &thumbnail {
        storage.write_thumbnail(&id, bytes).await?;
    }

    Ok(StoredFile {
        id,
        ext: output.ext.to_string(),
        existed: false,
        actions: describe_actions(
            source_ext,
            output.ext,
            (source_width, source_height),
            (output.width, output.height),
            output.applied_quality,
        ),
        has_thumbnail: thumbnail.is_some(),
    })
}

/// Strip EXIF (and other sidecar metadata) from an image container without
/// re-encoding the pixels. JPEG, PNG and WebP carry EXIF; anything else
/// (e.g. GIF) is returned unchanged.
fn strip_exif(input: Vec<u8>) -> Result<Vec<u8>> {
    use img_parts::{DynImage, ImageEXIF};

    let bytes = img_parts::Bytes::from(input);
    match DynImage::from_bytes(bytes.clone())
        .map_err(|e| Error::BadRequest(format!("could not parse image to strip EXIF: {e}")))?
    {
        Some(mut image) => {
            image.set_exif(None);
            let mut output = Vec::new();
            image
                .encoder()
                .write_to(&mut output)
                .map_err(|e| Error::Io(std::io::Error::other(format!("EXIF strip failed: {e}"))))?;
            Ok(output)
        }
        // Not an EXIF-bearing container (GIF, or an unrecognized wrapper): nothing to strip.
        None => Ok(bytes.to_vec()),
    }
}

/// Build a `StoredFile` for a dedup or skip hit from a stored record.
fn existing_stored(record: files::FileRecord) -> StoredFile {
    StoredFile {
        id: record.id,
        existed: true,
        actions: describe_actions(
            &record.original_ext,
            &record.ext,
            (record.original_width, record.original_height),
            (record.width, record.height),
            record.applied_quality,
        ),
        ext: record.ext,
        has_thumbnail: record.has_thumbnail,
    }
}

/// SHA-256 of a file's bytes, as a hex string. Streamed so large files never
/// buffer in memory.
async fn hash_file(path: &std::path::Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Format a byte count with a human-readable binary unit (e.g. "1.5 MiB").
fn human_readable_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Reject files above the configured byte limit (0 = unlimited).
pub fn check_size(settings: &ElysiaSettings, size: u64) -> Result<()> {
    let limit = settings.max_file_size_bytes;
    if limit > 0 && size > limit {
        return Err(Error::BadRequest(format!(
            "file too large: {}, limit is {}",
            human_readable_size(size),
            human_readable_size(limit)
        )));
    }
    Ok(())
}

/// Reject images above the configured pixel limits (0 = unlimited, per axis).
pub fn check_dimensions(settings: &ElysiaSettings, width: u32, height: u32) -> Result<()> {
    let max_width = settings.max_width_pixels;
    let max_height = settings.max_height_pixels;
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

/// Whether an image exceeds the thumbnail box on a constrained axis (0 = off).
/// When it does not, no separate thumbnail is made and /t/ serves the main file.
fn over_thumbnail_box((box_width, box_height): (u32, u32), width: u32, height: u32) -> bool {
    (box_width > 0 && width > box_width) || (box_height > 0 && height > box_height)
}

/// Pipeline options for a thumbnail: fit the box, lossy WebP.
fn thumbnail_process_options(settings: &ElysiaSettings) -> image::ProcessOptions {
    image::ProcessOptions {
        target_width: settings.thumbnail_width_pixels,
        target_height: settings.thumbnail_height_pixels,
        format: TargetFormat::Webp,
        target_file_size: 0,
        requested_quality: Some(settings.thumbnail_quality),
    }
}

/// Map an image pipeline error to the HTTP-facing error.
fn process_error(error: image::ProcessError) -> Error {
    match error {
        image::ProcessError::Undecodable => Error::UnsupportedMediaType,
        image::ProcessError::Encode(message) => Error::BadRequest(message),
    }
}

/// Resize the source into the thumbnail box and encode it as lossy WebP.
/// `Ok(None)` when the pipeline passes the source through (animated GIF).
async fn generate_thumbnail(source: Vec<u8>, settings: &ElysiaSettings) -> Result<Option<Vec<u8>>> {
    let options = thumbnail_process_options(settings);
    // Encoding is CPU-heavy; keep it off the async worker threads.
    let processed = tokio::task::spawn_blocking(move || image::process(&source, &options))
        .await
        .map_err(|_| Error::Io(std::io::Error::other("thumbnail task failed")))?
        .map_err(process_error)?;
    Ok(processed.map(|processed| processed.bytes))
}

/// Look up a file's metadata by id, or `NotFound`. The row doubles as a
/// path-traversal guard: only ids that exist are ever served.
pub async fn get(pool: &PgPool, id: &str) -> Result<FileRow> {
    files::find(pool, id).await?.ok_or(Error::NotFound)
}

#[cfg(test)]
mod tests {
    use super::human_readable_size;

    #[test]
    fn formats_human_readable_size() {
        assert_eq!(human_readable_size(512), "512 B");
        assert_eq!(human_readable_size(1024), "1.0 KiB");
        assert_eq!(human_readable_size(1536), "1.5 KiB");
        assert_eq!(human_readable_size(5 * 1024 * 1024), "5.0 MiB");
    }
}
