use actix_files::NamedFile;
use actix_web::web::{ServiceConfig, get, post};
use actix_web::{HttpRequest, HttpResponse};

use crate::error::{Error, Result};
use crate::settings::{RefererHost, SharedElysiaSettings, referer_host};

pub mod empty_trash;
pub mod list_images;
pub mod metadata;
pub mod restore_images;
pub mod serve;
pub mod serve_thumbnail;
pub mod trash_images;
pub mod upload;
pub mod upload_url;

/// Routes under `/api`.
pub fn config(cfg: &mut ServiceConfig) {
    cfg.route("/upload", post().to(upload::upload))
        .route("/upload-url", post().to(upload_url::upload_url))
        .route("/images", get().to(list_images::images))
        .route("/images/trash", post().to(trash_images::trash_images))
        .route("/images/trash/empty", post().to(empty_trash::empty_trash))
        .route(
            "/images/trash/restore",
            post().to(restore_images::restore_images),
        )
        .route("/i/{id}", get().to(metadata::metadata));
}

/// Public image links, outside `/api`.
pub fn config_public(cfg: &mut ServiceConfig) {
    cfg.route("/i/{filename}", get().to(serve::serve))
        .route("/t/{filename}", get().to(serve_thumbnail::serve_thumbnail));
}

/// Split a `/i/` or `/t/` path segment `<id>.<ext>` into its id and extension.
/// Both must be plain ASCII alphanumerics, so the pair can only name a stored
/// file: no path traversal, no temp or thumbnail file names.
pub fn parse_filename(filename: &str) -> Result<(&str, &str)> {
    let alphanumeric =
        |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric());
    match filename.split_once('.') {
        // The id needs 2 chars for its shard dir.
        Some((id, ext)) if id.len() >= 2 && alphanumeric(id) && alphanumeric(ext) => Ok((id, ext)),
        _ => Err(Error::NotFound),
    }
}

/// How an allowed image request is served.
pub(crate) struct ServeDecision {
    /// `private` while an allowlist is set, so shared caches (CDN, proxy) can't
    /// hand the image to other referrers.
    pub cache_control: &'static str,
    /// True when the `Referer` is one of Elysia's own pages (browse, viewer):
    /// browsing the app does not inflate the view counters, and still shows
    /// trashed images.
    pub from_own_page: bool,
}

/// Refuse an image request whose `Referer` is not in the allowed referrers.
pub(crate) fn check_referrer(
    req: &HttpRequest,
    settings: &SharedElysiaSettings,
) -> Result<ServeDecision> {
    let referer = req
        .headers()
        .get(actix_web::http::header::REFERER)
        .and_then(|value| value.to_str().ok());
    let own_host = req.connection_info().host().to_owned();
    let settings = settings.read().expect("settings lock poisoned");
    if !settings.referrer_allowed(referer, &own_host) {
        return Err(Error::Forbidden);
    }
    Ok(ServeDecision {
        cache_control: if settings.allowed_referrers.is_empty() {
            "public, max-age=31536000, immutable"
        } else {
            "private, max-age=31536000, immutable"
        },
        from_own_page: matches!(referer_host(referer, &own_host), Some(RefererHost::Own)),
    })
}

/// Serve a file missing from the served tree from the trash instead, uncached,
/// to Elysia's own pages only: other sites keep getting a 404.
pub(crate) fn trashed_response(
    req: &HttpRequest,
    decision: &ServeDecision,
    trashed_file: std::io::Result<NamedFile>,
) -> Result<HttpResponse> {
    match trashed_file {
        Ok(file) if decision.from_own_page => Ok(file_response(req, file, "private, no-store")),
        _ => Err(Error::NotFound),
    }
}

/// Turn a NamedFile into a long-lived, immutable response with the given
/// `Cache-Control`. The content type comes from the file extension, which
/// Elysia picked itself when storing the file.
pub(crate) fn file_response(
    req: &HttpRequest,
    file: NamedFile,
    cache_control: &'static str,
) -> HttpResponse {
    // NamedFile streams the bytes (no full read into RAM) and handles range
    // requests + conditional GET (ETag/Last-Modified) for free.
    let mut res = file.into_response(req);
    res.headers_mut().insert(
        actix_web::http::header::CACHE_CONTROL,
        actix_web::http::header::HeaderValue::from_static(cache_control),
    );
    res
}
