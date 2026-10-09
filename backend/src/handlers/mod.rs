use actix_files::NamedFile;
use actix_web::{HttpRequest, HttpResponse};

pub mod login;
pub mod serve;
pub mod serve_thumbnail;
pub mod settings;
pub mod upload;
pub mod upload_url;

pub use login::login;
pub use serve::serve;
pub use serve_thumbnail::serve_thumbnail;
pub use settings::{get_elysia_settings, get_public_elysia_settings, put_elysia_settings};
pub use upload::upload;
pub use upload_url::upload_url;

/// Strip the optional `.<ext>` suffix from a `/i/` or `/t/` path segment,
/// leaving the file id.
pub(crate) fn id_from_filename(filename: &str) -> &str {
    filename.rsplit_once('.').map_or(filename, |(id, _)| id)
}

/// Turn a NamedFile into a long-lived, immutable response, overriding the
/// content type with the given MIME (trusted over NamedFile's extension guess).
pub(crate) fn file_response(req: &HttpRequest, file: NamedFile, mime: &str) -> HttpResponse {
    // NamedFile streams the bytes (no full read into RAM) and handles range
    // requests + conditional GET (ETag/Last-Modified) for free.
    let mut res = file.into_response(req);
    res.headers_mut().insert(
        actix_web::http::header::CACHE_CONTROL,
        actix_web::http::header::HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    if let Ok(mime) = mime.parse() {
        res.headers_mut()
            .insert(actix_web::http::header::CONTENT_TYPE, mime);
    }
    res
}
