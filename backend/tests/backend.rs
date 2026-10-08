//! Tests for the pure logic and the upload endpoint: config, errors, storage
//! paths, id generation, the endpoint's size/dimension limits and full store
//! path, and the repository layer. The DB-backed tests run against a real
//! Postgres database via `#[sqlx::test]`.

use actix_multipart::form::{MultipartFormConfig, tempfile::TempFileConfig};
use actix_web::http::StatusCode;
use actix_web::http::header::CONTENT_TYPE;
use actix_web::test as actix_test;
use actix_web::web::Data;
use actix_web::{App, ResponseError, web};
use sqlx::PgPool;

use elysia::config::{AuthConfig, Config, DatabaseConfig, ServerConfig, StorageConfig};
use elysia::error::Error;
use elysia::handlers::upload;
use elysia::repository::files::{self, NewFile};
use elysia::services::files::{check_dimensions, check_size, generate_id, store_from_url};
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
            max_width_pixels: 0,
            max_height_pixels: 0,
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
fn check_size_enforces_limit() {
    let mut config = test_config();
    config.storage.max_file_size_bytes = 100;
    assert!(check_size(&config, 100).is_ok()); // at the limit is fine
    assert!(matches!(
        check_size(&config, 101),
        Err(Error::BadRequest(_))
    ));
    config.storage.max_file_size_bytes = 0; // 0 = unlimited
    assert!(check_size(&config, u64::MAX).is_ok());
}

#[test]
fn check_dimensions_enforces_each_axis() {
    let mut config = test_config();
    config.storage.max_width_pixels = 800;
    config.storage.max_height_pixels = 600;
    assert!(check_dimensions(&config, 800, 600).is_ok()); // at the limit is fine
    assert!(matches!(
        check_dimensions(&config, 801, 600),
        Err(Error::BadRequest(_))
    ));
    assert!(matches!(
        check_dimensions(&config, 800, 601),
        Err(Error::BadRequest(_))
    ));
    config.storage.max_width_pixels = 0; // 0 = unlimited, per axis
    assert!(check_dimensions(&config, u32::MAX, 600).is_ok());
}

/// PNG/zlib CRC-32 (IEEE), computed over the chunk type + data.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Smallest thing `infer` reads as a PNG and `imagesize` reads dimensions
/// from: the 8-byte signature plus a valid IHDR chunk, no image data.
fn tiny_png(width: u32, height: u32) -> Vec<u8> {
    let mut ihdr = b"IHDR".to_vec();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // bit depth, color type, compression, filter, interlace

    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13u32.to_be_bytes()); // IHDR data length
    bytes.extend_from_slice(&ihdr);
    bytes.extend_from_slice(&crc32(&ihdr).to_be_bytes());
    bytes
}

/// POST `image` to `/api/upload` against an app built from `config` and `pool`,
/// returning the status.
async fn upload_status(pool: PgPool, config: Config, image: Vec<u8>) -> StatusCode {
    let upload_dir = std::env::temp_dir().join(format!("elysia-test-{}", generate_id()));
    std::fs::create_dir_all(&upload_dir).unwrap();
    let token = config.auth.admin_token.clone();
    let storage = Storage::new(upload_dir.to_str().unwrap());

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(config))
            .app_data(Data::new(pool))
            .app_data(Data::new(storage))
            .app_data(MultipartFormConfig::default().total_limit(10 * 1024 * 1024))
            .app_data(TempFileConfig::default().directory(&upload_dir))
            .route("/api/upload", web::post().to(upload)),
    )
    .await;

    let boundary = "testboundary";
    let mut body = format!("--{boundary}\r\n").into_bytes();
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"x.png\"\r\n\
          Content-Type: image/png\r\n\r\n",
    );
    body.extend_from_slice(&image);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let request = actix_test::TestRequest::post()
        .uri("/api/upload")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(body)
        .to_request();

    let status = actix_test::call_service(&app, request).await.status();
    let _ = std::fs::remove_dir_all(&upload_dir);
    status
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rejects_file_over_size_limit(pool: PgPool) {
    let mut config = test_config();
    config.storage.max_file_size_bytes = 5; // the png is 33 bytes
    assert_eq!(
        upload_status(pool, config, tiny_png(2, 2)).await,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rejects_image_over_width_limit(pool: PgPool) {
    // Width over, height unlimited: isolates the width check.
    let mut config = test_config();
    config.storage.max_width_pixels = 1; // the image is 2 wide
    assert_eq!(
        upload_status(pool, config, tiny_png(2, 1)).await,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rejects_image_over_height_limit(pool: PgPool) {
    // Height over, width unlimited: isolates the height check.
    let mut config = test_config();
    config.storage.max_height_pixels = 1; // the image is 2 tall
    assert_eq!(
        upload_status(pool, config, tiny_png(1, 2)).await,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_within_limits_succeeds(pool: PgPool) {
    // Under both limits: validation passes, the row inserts, and the file is
    // stored, so the endpoint returns 200.
    let mut config = test_config();
    config.storage.max_file_size_bytes = 1024;
    config.storage.max_width_pixels = 16;
    config.storage.max_height_pixels = 16;
    assert_eq!(
        upload_status(pool, config, tiny_png(2, 2)).await,
        StatusCode::OK
    );
}

/// One-shot HTTP server on a loopback port: answers a single request with the
/// given status line, content type and body, then closes. Returns its URL.
fn serve_once(status_line: &'static str, content_type: &'static str, body: Vec<u8>) -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.read(&mut [0u8; 1024]); // drain the request line/headers
            let header = format!(
                "{status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    format!("http://{addr}/image.png")
}

/// Build a Storage over a fresh temp dir for rehost tests.
fn temp_storage() -> (Storage, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("elysia-test-{}", generate_id()));
    std::fs::create_dir_all(&dir).unwrap();
    (Storage::new(dir.to_str().unwrap()), dir)
}

#[sqlx::test(migrations = "./migrations")]
async fn rehost_stores_a_fetched_image(pool: PgPool) {
    let (storage, dir) = temp_storage();
    let url = serve_once("HTTP/1.1 200 OK", "image/png", tiny_png(2, 2));
    let stored = store_from_url(&pool, &storage, &test_config(), &url)
        .await
        .unwrap();
    assert_eq!(stored.ext, "png");
    assert!(storage.path_for(&stored.id, "png").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[sqlx::test(migrations = "./migrations")]
async fn identical_uploads_are_deduplicated(pool: PgPool) {
    // Same bytes stored twice: the second reuses the first id and sets existed,
    // and no second copy lands on disk.
    let (storage, dir) = temp_storage();
    let config = test_config();
    let image = tiny_png(2, 2);

    let first = store_from_url(
        &pool,
        &storage,
        &config,
        &serve_once("HTTP/1.1 200 OK", "image/png", image.clone()),
    )
    .await
    .unwrap();
    assert!(!first.existed);

    let second = store_from_url(
        &pool,
        &storage,
        &config,
        &serve_once("HTTP/1.1 200 OK", "image/png", image.clone()),
    )
    .await
    .unwrap();
    assert!(second.existed);
    assert_eq!(first.id, second.id);

    let stored: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .collect();
    assert_eq!(stored.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[sqlx::test(migrations = "./migrations")]
async fn rehost_rejects_a_non_image_link(pool: PgPool) {
    let (storage, dir) = temp_storage();
    let url = serve_once("HTTP/1.1 200 OK", "text/plain", b"not an image".to_vec());
    assert!(matches!(
        store_from_url(&pool, &storage, &test_config(), &url).await,
        Err(Error::UnsupportedMediaType)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[sqlx::test(migrations = "./migrations")]
async fn rehost_rejects_a_failed_fetch(pool: PgPool) {
    let (storage, dir) = temp_storage();
    let url = serve_once("HTTP/1.1 404 Not Found", "text/plain", b"nope".to_vec());
    assert!(matches!(
        store_from_url(&pool, &storage, &test_config(), &url).await,
        Err(Error::BadRequest(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
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

fn sample_file(id: &str) -> NewFile<'_> {
    NewFile {
        id,
        ext: "png",
        mime: "image/png",
        original_name: Some("x.png"),
        size: 33,
        width: 2,
        height: 2,
        // Hash tied to the id so distinct ids get distinct (unique) hashes.
        hash: id,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn insert_is_collision_safe_and_find_roundtrips(pool: PgPool) {
    // First insert of an id succeeds.
    assert!(
        files::insert(&pool, &sample_file("abc12345"))
            .await
            .unwrap()
    );
    // Re-inserting the same id hits the unique constraint: Ok(false), not an error.
    assert!(
        !files::insert(&pool, &sample_file("abc12345"))
            .await
            .unwrap()
    );

    // The row reads back with the columns it was stored with.
    let row = files::find(&pool, "abc12345").await.unwrap().unwrap();
    assert_eq!(row.id, "abc12345");
    assert_eq!(row.ext, "png");
    assert_eq!(row.mime, "image/png");

    // A missing id is None, not an error.
    assert!(files::find(&pool, "missing0").await.unwrap().is_none());
}
