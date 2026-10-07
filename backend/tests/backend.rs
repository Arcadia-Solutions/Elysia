//! Unit tests for the no-database logic: config, errors, storage paths, id
//! generation, and the two handlers/extractors that only read the admin token.
//! Anything touching Postgres is left to DB-backed tests (CI has no database).

use actix_web::ResponseError;
use actix_web::http::StatusCode;
use actix_web::http::header::CONTENT_TYPE;

use elysia::config::{AuthConfig, Config, DatabaseConfig, ServerConfig, StorageConfig};
use elysia::error::Error;
use elysia::services::files::generate_id;
use elysia::storage::Storage;

fn test_config() -> Config {
    Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 8080,
        },
        database: DatabaseConfig {
            host: "localhost".into(),
            port: 5432,
            user: "elysia".into(),
            password: "secret".into(),
            name: "elysia".into(),
        },
        storage: StorageConfig {
            upload_dir: "/tmp/elysia".into(),
            max_file_size_bytes: 0,
        },
        auth: AuthConfig {
            admin_token: "admin-token".into(),
        },
    }
}

#[test]
fn database_url_formats_dsn() {
    assert_eq!(
        test_config().database.url(),
        "postgres://elysia:secret@localhost:5432/elysia"
    );
}

#[test]
fn error_status_codes_map() {
    assert_eq!(
        Error::BadRequest("x".into()).status_code(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(Error::Unauthorized.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(Error::NotFound.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(
        Error::UnsupportedMediaType.status_code(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        Error::Io(std::io::Error::other("boom")).status_code(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn error_response_is_json() {
    let res = Error::NotFound.error_response();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let content_type = res.headers().get(CONTENT_TYPE).unwrap();
    assert_eq!(content_type, "application/json");
}

#[test]
fn storage_path_joins_root_id_ext() {
    let storage = Storage::new("/var/uploads");
    assert_eq!(
        storage.path_for("abc12345", "png"),
        std::path::PathBuf::from("/var/uploads/abc12345.png")
    );
}

#[test]
fn storage_new_trims_trailing_slashes() {
    // trailing slashes stripped so path_for never produces a doubled separator
    let storage = Storage::new("/var/uploads///");
    assert_eq!(
        storage.path_for("x", "jpg"),
        std::path::PathBuf::from("/var/uploads/x.jpg")
    );
}

#[test]
fn generate_id_is_8_base62_chars() {
    let id = generate_id();
    assert_eq!(id.chars().count(), 8);
    assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn generate_id_varies_between_calls() {
    // a repeat across two draws means a broken RNG, not bad luck (62^8 space)
    assert_ne!(generate_id(), generate_id());
}
