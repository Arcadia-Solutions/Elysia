use std::io::Write;

use ::image::metadata::Orientation;
use serde::Serialize;
use sqlx::PgPool;
use tempfile::{NamedTempFile, TempPath};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::repository::files::{self, NewFile};
use crate::services::image::{self, TargetFormat};
use crate::services::paginate;
use crate::settings::ElysiaSettings;
use crate::storage::Storage;

/// Public file id: 8 base62 characters. nanoid's default alphabet also has
/// `_` and `-`, which look odd in links.
pub fn generate_id() -> String {
    const BASE62: [char; 62] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h',
        'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
        'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R',
        'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
    ];
    nanoid::nanoid!(8, &BASE62)
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
    pub fn image_url(&self, public_url: &str) -> String {
        image_url(public_url, &self.id, &self.ext)
    }

    /// Public URL for the thumbnail. Generated thumbnails are always WebP;
    /// without one, `/t/` falls back to the original, hence its extension.
    pub fn thumbnail_url(&self, public_url: &str) -> String {
        thumbnail_url(public_url, &self.id, &self.ext, self.has_thumbnail)
    }
}

/// Public URL for a stored image, under `public_url` (the configured
/// `server.public_url`, without a trailing slash).
pub fn image_url(public_url: &str, id: &str, ext: &str) -> String {
    format!("{public_url}/i/{id}.{ext}")
}

/// Images returned per browse page.
pub const IMAGES_PER_PAGE: i64 = 100;

/// Thumbnail URL for a trashed file, served admin-only from the trash.
pub fn trashed_thumbnail_url(public_url: &str, id: &str, ext: &str) -> String {
    format!("{public_url}/api/images/trash/{id}.{ext}")
}

/// Public thumbnail URL for a stored file. Generated thumbnails are always
/// WebP; without one, `/t/` falls back to the original, hence its extension.
pub fn thumbnail_url(public_url: &str, id: &str, ext: &str, has_thumbnail: bool) -> String {
    let extension = if has_thumbnail { "webp" } else { ext };
    format!("{public_url}/t/{id}.{extension}")
}

/// Fetch one page (1-based) of images matching the query, only `uploader`'s
/// when set, with the total count. An out-of-range page returns no rows but still reports the real
/// total, so callers can show the pager.
pub async fn list_images(
    pool: &PgPool,
    query: &files::ImagesQuery,
    uploader: Option<Uuid>,
) -> Result<(Vec<files::ImageListRow>, i64)> {
    paginate(
        query.page,
        IMAGES_PER_PAGE,
        |offset| files::list_page(pool, offset, IMAGES_PER_PAGE, query, uploader),
        files::count(pool, query.status, uploader),
        |row| row.total,
    )
    .await
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
    Lossless(&'static str),
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
            None => CompressionAction::Lossless("lossless"),
        },
    })
}

/// Client for user-submitted URL uploads, shared so connections are reused.
/// It only connects to public addresses: the resolver drops every non-public
/// address a hostname resolves to (so DNS rebinding gets nothing to connect
/// to) and each redirect hop goes through [`check_public_url`], since IP
/// literal hosts skip the resolver. Callers check the first URL themselves.
/// The admin-run Chevereto importer and the tests use a plain client instead,
/// as they legitimately fetch from loopback or LAN hosts.
pub fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            // A proxy would resolve and connect in our place, past the guard.
            .no_proxy()
            .dns_resolver(std::sync::Arc::new(PublicResolver))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    attempt.error("too many redirects")
                } else if let Err(e) = check_public_url(attempt.url()) {
                    attempt.error(e)
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .expect("failed to build http client")
    });
    &CLIENT
}

/// A URL the public rehost route refuses to fetch.
#[derive(Debug, thiserror::Error)]
#[error("url must be http(s) and point to a public address")]
pub struct BlockedUrl;

/// Reject a non-http(s) URL, or one whose host is a non-public IP literal.
/// Hostnames pass here; [`http_client`]'s resolver filters what they resolve to.
pub fn check_public_url(url: &url::Url) -> std::result::Result<(), BlockedUrl> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(BlockedUrl);
    }
    match url.host() {
        Some(url::Host::Domain(_)) => Ok(()),
        Some(url::Host::Ipv4(ip)) if is_public_ip(ip.into()) => Ok(()),
        Some(url::Host::Ipv6(ip)) if is_public_ip(ip.into()) => Ok(()),
        _ => Err(BlockedUrl),
    }
}

/// Resolver that keeps only public addresses, see [`http_client`].
struct PublicResolver;

impl reqwest::dns::Resolve for PublicResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let addrs: Vec<_> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|addr| is_public_ip(addr.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(BlockedUrl.into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Whether `ip` is a globally routable unicast address. Spelled out because
/// `IpAddr::is_global` is still unstable.
fn is_public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || a == 0 // "this network"
                || a >= 240 // reserved
                || (a == 100 && b & 0xc0 == 64) // CGNAT 100.64/10
                || (a == 192 && b == 0 && c == 0) // IETF protocol assignments
                || (a == 198 && b & 0xfe == 18)) // benchmarking 198.18/15
        }
        std::net::IpAddr::V6(ip) => {
            // IPv4-mapped and IPv4-compatible (this covers :: and ::1 too).
            if let Some(v4) = ip.to_ipv4() {
                return is_public_ip(v4.into());
            }
            let [a, b, ..] = ip.segments();
            !(ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || a & 0xffc0 == 0xfec0 // deprecated site-local
                || (a == 0x2001 && b == 0xdb8) // documentation
                || (a == 0x64 && b == 0xff9b) // NAT64, embeds an IPv4 address
                || a == 0x2002) // 6to4, embeds an IPv4 address
        }
    }
}

/// Fetch an image from `url` and store it, sharing the multipart upload's
/// validation. `original_name` defaults to the URL's last path segment. Returns
/// a `BadRequest` when the URL cannot be fetched, and the usual
/// `UnsupportedMediaType` when the fetched bytes are not a supported image.
#[allow(clippy::too_many_arguments)]
pub async fn store_from_url(
    pool: &PgPool,
    storage: &Storage,
    settings: &ElysiaSettings,
    client: &reqwest::Client,
    url: &str,
    original_name: Option<String>,
    options: &UploadOptions,
    uploaded_by_user_id: Uuid,
) -> Result<StoredFile> {
    let mut response = client.get(url).send().await.map_err(|e| {
        // The guard's refusal is buried in reqwest's error chain.
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&e);
        while let Some(error) = source {
            if error.is::<BlockedUrl>() {
                return Error::BadRequest(BlockedUrl.to_string());
            }
            source = error.source();
        }
        Error::BadRequest(format!("could not fetch url: {e}"))
    })?;
    if !response.status().is_success() {
        return Err(Error::BadRequest(format!(
            "url returned status {}",
            response.status().as_u16()
        )));
    }

    // Stream the body to disk, rejecting it as soon as it exceeds the limit.
    let mut temp = storage.new_temp()?;
    let mut size = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Error::BadRequest(format!("could not read url body: {e}")))?
    {
        size += chunk.len() as u64;
        check_size(settings, size)?;
        temp.write_all(&chunk)?;
    }

    let original_name = original_name.or_else(|| {
        url.rsplit('/')
            .next()
            .map(|s| s.split(['?', '#']).next().unwrap_or(s).to_string())
            .filter(|s| !s.is_empty())
    });

    store_upload(
        pool,
        storage,
        settings,
        temp,
        size,
        original_name,
        options,
        uploaded_by_user_id,
    )
    .await
}

/// Validate, inspect and store an uploaded temp file: size check, type sniff,
/// dimensions, a collision-safe row insert, then persist to disk. Shared by the
/// multipart upload and the URL rehost.
#[allow(clippy::too_many_arguments)]
pub async fn store_upload(
    pool: &PgPool,
    storage: &Storage,
    settings: &ElysiaSettings,
    temp: NamedTempFile,
    size: u64,
    original_name: Option<String>,
    options: &UploadOptions,
    uploaded_by_user_id: Uuid,
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
    // Browsers and the pipeline apply the EXIF orientation, so record the upright size.
    let orientation = image::orientation(path);
    let (source_width, source_height) = match orientation {
        Orientation::Rotate90
        | Orientation::Rotate270
        | Orientation::Rotate90FlipH
        | Orientation::Rotate270FlipH => (dimensions.height as i32, dimensions.width as i32),
        _ => (dimensions.width as i32, dimensions.height as i32),
    };
    let original_hash = hash_file(path).await?;
    let requested_quality = options.lossy_compression_value.map(|q| q as i32);

    // Same source under the same requested quality was already processed.
    // Note: the key is (original_hash, requested_quality) only, ignoring the
    // target format/size/dimensions config. If an operator changes that config,
    // a re-upload of the same source returns the file processed under the old config.
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
    // Passthrough stores the source, so the output matches the source dimensions.
    let passthrough_thumbnail = thumbnails_enabled
        && target_file_format.is_none()
        && over_thumbnail_box(thumbnail_box, source_width as u32, source_height as u32);
    // Single read shared (refcounted, not copied) by the pipeline, EXIF strip and
    // thumbnail below; a plain passthrough needs none of them and skips it.
    let source_bytes: img_parts::Bytes =
        if target_file_format.is_some() || options.strip_exif || passthrough_thumbnail {
            tokio::fs::read(path).await?.into()
        } else {
            img_parts::Bytes::new()
        };

    // The processed main output (`None` = passthrough or animated GIF) and, when
    // the target-format pipeline runs, its thumbnail decoded from the same pixels.
    let (processed, mut thumbnail) = match target_file_format {
        Some(format) => {
            let input = source_bytes.clone();
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
            let permit = processing_permit().await;
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let Some((image, icc_profile)) = image::decode(&input)? else {
                    return Ok::<_, image::ProcessError>((None, None));
                };
                let source = thumbnail_options.is_some().then(|| image.clone());
                let processed =
                    image::process_decoded(image, icc_profile.as_deref(), &process_options)?;
                let thumbnail = match (thumbnail_options, source) {
                    (Some(options), Some(source))
                        if over_thumbnail_box(thumbnail_box, processed.width, processed.height) =>
                    {
                        Some(
                            image::process_decoded(source, icc_profile.as_deref(), &options)?.bytes,
                        )
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
        // drops EXIF (re-encode starts from a bare pixel buffer), so EXIF is
        // only stripped here, when requested, by rewriting the container.
        None if options.strip_exif => {
            let stripped = strip_exif(source_bytes.clone(), orientation)?;
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
    if passthrough_thumbnail {
        thumbnail = generate_thumbnail(source_bytes, settings).await?;
    }

    let hash = hash_file(output.temp.path()).await?;

    // A different source that produced identical output bytes.
    if let Some(record) = files::find_record_by_hash(pool, &hash).await? {
        return Ok(existing_stored(record));
    }

    let original_name = original_name.as_deref();

    // Files hit disk before the row exists, so a failed persist never leaves a
    // row with no file. On an id/hash conflict the just-written files are
    // removed again before retrying.
    const ID_ALLOC_RETRIES: u8 = 5;
    let mut id = None;
    // Renamed into place, never copied: the temp file lives under the storage root.
    let mut temp = output.temp.into_temp_path();
    for _ in 0..ID_ALLOC_RETRIES {
        let candidate = generate_id();
        storage.prepare_shard(&candidate).await?;
        let dest = storage.path_for(&candidate, output.ext);
        temp.persist(&dest).map_err(|error| error.error)?;
        if let Some(bytes) = &thumbnail
            && let Err(error) = storage.write_thumbnail(&candidate, bytes).await
        {
            let _ = tokio::fs::remove_file(&dest).await;
            return Err(error);
        }
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
                uploaded_by_user_id,
            },
        )
        .await;
        match inserted {
            Ok(true) => {
                id = Some(candidate);
                break;
            }
            Ok(false) => {
                // Keep the bytes for the next candidate; deleted on drop if none succeeds.
                temp = TempPath::try_from_path(dest)?;
                let _ = tokio::fs::remove_file(storage.thumbnail_path_for(&candidate)).await;
                // A failed insert is an id collision or a concurrent upload of the same
                // bytes winning the hash race; in the latter case, reuse its row.
                if let Some(record) = files::find_record_by_hash(pool, &hash).await? {
                    return Ok(existing_stored(record));
                }
            }
            Err(error) => {
                let _ = tokio::fs::remove_file(&dest).await;
                let _ = tokio::fs::remove_file(storage.thumbnail_path_for(&candidate)).await;
                return Err(error);
            }
        }
    }
    let id = id.ok_or(Error::Internal)?;
    crate::telemetry::record_upload(output.ext, size, output.size as u64);

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
fn strip_exif(bytes: img_parts::Bytes, orientation: Orientation) -> Result<Vec<u8>> {
    use img_parts::{DynImage, ImageEXIF};

    match DynImage::from_bytes(bytes.clone())
        .map_err(|e| Error::BadRequest(format!("could not parse image to strip EXIF: {e}")))?
    {
        Some(mut image) => {
            // The pixels are stored unrotated, so dropping the Orientation tag would
            // show them sideways: keep a minimal EXIF with only that tag rather
            // than rotate, which would mean a lossy re-encode.
            image.set_exif(
                (orientation != Orientation::NoTransforms).then(|| orientation_exif(orientation)),
            );
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

/// Minimal little-endian EXIF (TIFF) block holding only the Orientation tag.
fn orientation_exif(orientation: Orientation) -> img_parts::Bytes {
    let mut exif = b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
    exif.extend_from_slice(&[orientation.to_exif(), 0, 0, 0, 0, 0, 0, 0]);
    exif.into()
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

/// Wait for a slot to run an image decode/encode job. Uploads queue here rather
/// than fail, so concurrent large encodes cannot exhaust CPU and memory.
// ponytail: one global limit of one job per core, shared by uploads and imports
// alike; add per-user fairness if a single uploader can starve the others.
async fn processing_permit() -> tokio::sync::SemaphorePermit<'static> {
    static PERMITS: std::sync::LazyLock<tokio::sync::Semaphore> = std::sync::LazyLock::new(|| {
        tokio::sync::Semaphore::new(std::thread::available_parallelism().map_or(1, |n| n.get()))
    });
    PERMITS
        .acquire()
        .await
        .expect("processing semaphore is never closed")
}

/// Resize the source into the thumbnail box and encode it as lossy WebP.
/// `Ok(None)` when the pipeline passes the source through (animated GIF).
async fn generate_thumbnail(
    source: img_parts::Bytes,
    settings: &ElysiaSettings,
) -> Result<Option<Vec<u8>>> {
    let options = thumbnail_process_options(settings);
    // Encoding is CPU-heavy; keep it off the async worker threads.
    let permit = processing_permit().await;
    let processed = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        image::process(&source, &options)
    })
    .await
    .map_err(|_| Error::Io(std::io::Error::other("thumbnail task failed")))?
    .map_err(process_error)?;
    Ok(processed.map(|processed| processed.bytes))
}

/// Whether a user may trash a file: admins always, its uploader while the file
/// is `recent` (within `files::UPLOADER_TRASH_WINDOW_MINUTES`).
pub fn may_trash(user_id: Uuid, is_admin: bool, uploaded_by_user_id: Uuid, recent: bool) -> bool {
    is_admin || (user_id == uploaded_by_user_id && recent)
}

/// Move files to the trash, where they wait 30 days before the scheduled
/// wipe. See `may_trash` for who may. Unknown or already trashed ids are skipped.
pub async fn trash(
    pool: &PgPool,
    storage: &Storage,
    user_id: Uuid,
    is_admin: bool,
    ids: &[String],
) -> Result<()> {
    let rows = files::find_available(pool, ids).await?;
    if !rows
        .iter()
        .all(|row| may_trash(user_id, is_admin, row.uploaded_by_user_id, row.recent))
    {
        return Err(Error::Forbidden);
    }
    // Files move before the rows flip: a failure part way leaves rows still
    // available, and a retry skips the files that already moved.
    for row in &rows {
        storage.trash(&row.id, &row.ext).await?;
    }
    let ids: Vec<String> = rows.into_iter().map(|row| row.id).collect();
    files::mark_trashed(pool, &ids, user_id).await
}

/// Days a trashed file waits before the scheduled wipe. Also spelled out as
/// "30 days" in the OpenAPI descriptions and the frontend strings: update them
/// together.
pub const TRASH_RETENTION_DAYS: i32 = 30;

/// Wipe the files trashed at least `days` days ago from disk (all of them for
/// 0), keeping their rows as `deleted` for the stats. `deleted_by_id` is the
/// admin emptying the trash, None for the scheduler. A file that fails to wipe
/// is logged and stays trashed, so it does not block the others.
pub async fn wipe_trash(
    pool: &PgPool,
    storage: &Storage,
    days: i32,
    deleted_by_id: Option<Uuid>,
) -> Result<()> {
    let mut wiped_ids = Vec::new();
    for row in files::trashed_before(pool, days).await? {
        match storage.wipe_trashed(&row.id, &row.ext).await {
            Ok(()) => wiped_ids.push(row.id),
            Err(error) => tracing::warn!("failed to wipe trashed file {}: {error:?}", row.id),
        }
    }
    files::mark_deleted(pool, &wiped_ids, deleted_by_id).await
}

/// Look up a file's display metadata by id, or `NotFound`.
pub async fn get_metadata(pool: &PgPool, id: &str) -> Result<files::FileMetadata> {
    files::find_metadata(pool, id).await?.ok_or(Error::NotFound)
}

#[cfg(test)]
mod tests {
    use super::{check_public_url, human_readable_size, is_public_ip};

    #[test]
    fn only_public_ips_pass() {
        for ip in [
            "0.0.0.0",
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "224.0.0.1",
            "255.255.255.255",
            "192.0.2.1",
            "198.18.0.1",
            "240.0.0.1",
            "192.0.0.8",
            "::",
            "::1",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::a00:1",
            "2002:a00:1::",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} should be blocked");
        }
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "100.128.0.1",
            "2606:4700::1111",
            "::ffff:1.1.1.1",
        ] {
            assert!(is_public_ip(ip.parse().unwrap()), "{ip} should pass");
        }
    }

    #[test]
    fn only_public_http_urls_pass() {
        for url in [
            "http://127.0.0.1/a.png",
            "http://2130706433/a.png",
            "http://0x7f.1/a.png",
            "http://[::1]/a.png",
            "http://[::ffff:7f00:1]/a.png",
            "https://169.254.169.254/latest/meta-data",
            "ftp://example.com/a.png",
            "file:///etc/passwd",
        ] {
            assert!(check_public_url(&url.parse().unwrap()).is_err(), "{url}");
        }
        for url in ["https://example.com/a.png", "http://1.1.1.1:8080/a.png"] {
            assert!(check_public_url(&url.parse().unwrap()).is_ok(), "{url}");
        }
    }

    #[test]
    fn formats_human_readable_size() {
        assert_eq!(human_readable_size(512), "512 B");
        assert_eq!(human_readable_size(1024), "1.0 KiB");
        assert_eq!(human_readable_size(1536), "1.5 KiB");
        assert_eq!(human_readable_size(5 * 1024 * 1024), "5.0 MiB");
    }
}
