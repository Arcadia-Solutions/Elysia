use actix_files::NamedFile;
use actix_web::{HttpRequest, HttpResponse};

use crate::error::{Error, Result};
use crate::settings::SharedElysiaSettings;

pub mod change_password;
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
pub mod sso_callback;
pub mod sso_login;
pub mod upload;
pub mod upload_url;

pub use change_password::change_password;
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
pub use sso_callback::sso_callback;
pub use sso_login::sso_login;
pub use upload::upload;
pub use upload_url::upload_url;

/// Strip the optional `.<ext>` suffix from a `/i/` or `/t/` path segment,
/// leaving the file id.
pub(crate) fn id_from_filename(filename: &str) -> &str {
    filename.rsplit_once('.').map_or(filename, |(id, _)| id)
}

/// Refuse an image request whose `Referer` is not in the allowed referrers.
/// Returns the `Cache-Control` to serve it with: `private` while an allowlist
/// is set, so shared caches (CDN, proxy) can't hand it to other referrers.
pub(crate) fn check_referrer(
    req: &HttpRequest,
    settings: &SharedElysiaSettings,
) -> Result<&'static str> {
    let referer = req
        .headers()
        .get(actix_web::http::header::REFERER)
        .and_then(|value| value.to_str().ok());
    let settings = settings.read().expect("settings lock poisoned");
    if !settings.referrer_allowed(referer, req.connection_info().host()) {
        return Err(Error::Forbidden);
    }
    Ok(if settings.allowed_referrers.is_empty() {
        "public, max-age=31536000, immutable"
    } else {
        "private, max-age=31536000, immutable"
    })
}

/// Turn a NamedFile into a long-lived, immutable response with the given
/// `Cache-Control`, overriding the content type with the given MIME (trusted
/// over NamedFile's extension guess).
pub(crate) fn file_response(
    req: &HttpRequest,
    file: NamedFile,
    mime: &str,
    cache_control: &'static str,
) -> HttpResponse {
    // NamedFile streams the bytes (no full read into RAM) and handles range
    // requests + conditional GET (ETag/Last-Modified) for free.
    let mut res = file.into_response(req);
    res.headers_mut().insert(
        actix_web::http::header::CACHE_CONTROL,
        actix_web::http::header::HeaderValue::from_static(cache_control),
    );
    if let Ok(mime) = mime.parse() {
        res.headers_mut()
            .insert(actix_web::http::header::CONTENT_TYPE, mime);
    }
    res
}
