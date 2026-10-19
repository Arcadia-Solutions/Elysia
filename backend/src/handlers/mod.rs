use actix_files::NamedFile;
use actix_web::{HttpRequest, HttpResponse};

use crate::error::{Error, Result};
use crate::settings::{RefererHost, SharedElysiaSettings, referer_host};

pub mod api_keys;
pub mod change_password;
pub mod empty_trash;
pub mod get_elysia_settings;
pub mod get_elysia_unauth_settings;
pub mod get_public_elysia_settings;
pub mod images;
pub mod login;
pub mod me;
pub mod metadata;
pub mod put_elysia_settings;
pub mod register;
pub mod serve;
pub mod serve_thumbnail;
pub mod serve_trashed_thumbnail;
pub mod sso_callback;
pub mod sso_login;
pub mod stats;
pub mod trash_images;
pub mod upload;
pub mod upload_url;
pub mod users;

pub use api_keys::{create_api_key, list_api_keys, revoke_api_key};
pub use change_password::change_password;
pub use empty_trash::empty_trash;
pub use get_elysia_settings::get_elysia_settings;
pub use get_elysia_unauth_settings::get_elysia_unauth_settings;
pub use get_public_elysia_settings::get_public_elysia_settings;
pub use images::images;
pub use login::login;
pub use me::me;
pub use metadata::metadata;
pub use put_elysia_settings::put_elysia_settings;
pub use register::register;
pub use serve::serve;
pub use serve_thumbnail::serve_thumbnail;
pub use serve_trashed_thumbnail::serve_trashed_thumbnail;
pub use sso_callback::sso_callback;
pub use sso_login::sso_login;
pub use stats::stats_overview;
pub use trash_images::trash_images;
pub use upload::upload;
pub use upload_url::upload_url;
pub use users::{get_user, list_users, set_user_banned};

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
    /// False when the `Referer` is one of Elysia's own pages (browse, viewer),
    /// so browsing the app does not inflate the view counters.
    pub count_view: bool,
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
        count_view: !matches!(referer_host(referer, &own_host), Some(RefererHost::Own)),
    })
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
