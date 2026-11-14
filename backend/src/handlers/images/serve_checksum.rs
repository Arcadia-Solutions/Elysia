use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};
use sqlx::PgPool;

use super::serve_preview::open_preview_or_original;
use super::{check_referrer, file_response};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::repository::checksums;
use crate::services::checksums::ChecksumAlgorithm;
use crate::services::views::ViewCounter;
use crate::settings::SharedElysiaSettings;
use crate::storage::{Preview, Storage};

#[utoipa::path(
    tag = "Images",
    get,
    path = "/ih/{algorithm}/{filename}",
    description = "Content-addressed link: serves the available image whose stored bytes \
        have this checksum, like `/i/`. SHA-256 checksums always exist, the other algorithms \
        once computed on upload or by the maintenance job. The extension is optional and \
        ignored, the digest case-insensitive. Trashed images are not served.",
    params(
        ("algorithm" = ChecksumAlgorithm, Path, description = "Checksum algorithm"),
        ("filename" = String, Path, description = "<hex digest>, optionally followed by .<ext>"),
    ),
    responses(
        (status = 200, description = "The image bytes"),
        (status = 403, description = "Referer not in the allowed referrers"),
        (status = 404, description = "No available image with this checksum"),
    )
)]
pub async fn serve_image_by_checksum(
    req: HttpRequest,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    views: Data<ViewCounter>,
    config: Data<Config>,
    settings: SharedElysiaSettings,
    path: Path<(ChecksumAlgorithm, String)>,
) -> Result<HttpResponse> {
    serve(
        &req, &pool, &storage, &views, &config, &settings, &path, None,
    )
    .await
}

#[utoipa::path(
    tag = "Images",
    get,
    path = "/mh/{algorithm}/{filename}",
    description = "The medium of the image whose stored bytes have this checksum, served \
        like `/m/`, keyed like `/ih/`.",
    params(
        ("algorithm" = ChecksumAlgorithm, Path, description = "Checksum algorithm of the image"),
        ("filename" = String, Path, description = "<hex digest of the image>, optionally followed by .<ext>"),
    ),
    responses(
        (status = 200, description = "The medium bytes, or the original image when it is smaller than the medium box"),
        (status = 403, description = "Referer not in the allowed referrers"),
        (status = 404, description = "No available image with this checksum"),
    )
)]
pub async fn serve_medium_by_checksum(
    req: HttpRequest,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    views: Data<ViewCounter>,
    config: Data<Config>,
    settings: SharedElysiaSettings,
    path: Path<(ChecksumAlgorithm, String)>,
) -> Result<HttpResponse> {
    let preview = Some(Preview::Medium);
    serve(
        &req, &pool, &storage, &views, &config, &settings, &path, preview,
    )
    .await
}

#[utoipa::path(
    tag = "Images",
    get,
    path = "/th/{algorithm}/{filename}",
    description = "The thumbnail of the image whose stored bytes have this checksum, served \
        like `/t/`, keyed like `/ih/`.",
    params(
        ("algorithm" = ChecksumAlgorithm, Path, description = "Checksum algorithm of the image"),
        ("filename" = String, Path, description = "<hex digest of the image>, optionally followed by .<ext>"),
    ),
    responses(
        (status = 200, description = "The thumbnail bytes, or the original image when it is within the thumbnail box"),
        (status = 403, description = "Referer not in the allowed referrers"),
        (status = 404, description = "No available image with this checksum"),
    )
)]
pub async fn serve_thumbnail_by_checksum(
    req: HttpRequest,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    views: Data<ViewCounter>,
    config: Data<Config>,
    settings: SharedElysiaSettings,
    path: Path<(ChecksumAlgorithm, String)>,
) -> Result<HttpResponse> {
    let preview = Some(Preview::Thumbnail);
    serve(
        &req, &pool, &storage, &views, &config, &settings, &path, preview,
    )
    .await
}

/// Serve the image with this checksum, or its preview, counting a view of it.
#[expect(clippy::too_many_arguments)]
async fn serve(
    req: &HttpRequest,
    pool: &PgPool,
    storage: &Storage,
    views: &ViewCounter,
    config: &Config,
    settings: &SharedElysiaSettings,
    (algorithm, filename): &(ChecksumAlgorithm, String),
    preview: Option<Preview>,
) -> Result<HttpResponse> {
    let decision = check_referrer(req, config.server.base_path(), settings)?;
    let digest = parse_digest(filename)?;
    let record = checksums::find_record(pool, *algorithm, &digest)
        .await?
        .ok_or(Error::NotFound)?;
    let file = match preview {
        None => storage.open(&record.id, &record.ext),
        Some(preview) => open_preview_or_original(storage, &record.id, &record.ext, preview),
    }
    .map_err(|_| Error::NotFound)?;
    views.count(&record.id, &decision.referrer, preview);
    Ok(file_response(req, file, decision.cache_control))
}

/// Lowercase hex digest out of `<digest>` or `<digest>.<ext>`.
fn parse_digest(filename: &str) -> Result<String> {
    let digest = filename
        .split_once('.')
        .map_or(filename, |(digest, _)| digest);
    if !digest.is_empty() && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(digest.to_ascii_lowercase())
    } else {
        Err(Error::NotFound)
    }
}
