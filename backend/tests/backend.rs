//! Tests for the pure logic and the upload endpoint: config, errors, storage
//! paths, id generation, the endpoint's size/dimension limits and full store
//! path, and the repository layer. The DB-backed tests run against a real
//! Postgres database via `#[sqlx::test]`.

mod common;

use actix_web::ResponseError;
use actix_web::http::StatusCode;
use actix_web::http::header::CONTENT_TYPE;
use sqlx::PgPool;

use elysia::error::Error;
use elysia::repository::files::{self, NewFile};
use elysia::services::files::{check_dimensions, check_size, generate_id, store_from_url};
use elysia::storage::Storage;

use common::*;

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
    let mut settings = test_settings();
    settings.max_file_size_bytes = 100;
    assert!(check_size(&settings, 100).is_ok()); // at the limit is fine
    assert!(matches!(
        check_size(&settings, 101),
        Err(Error::BadRequest(_))
    ));
    settings.max_file_size_bytes = 0; // 0 = unlimited
    assert!(check_size(&settings, u64::MAX).is_ok());
}

#[test]
fn check_dimensions_enforces_each_axis() {
    let mut settings = test_settings();
    settings.max_width_pixels = 800;
    settings.max_height_pixels = 600;
    assert!(check_dimensions(&settings, 800, 600).is_ok()); // at the limit is fine
    assert!(matches!(
        check_dimensions(&settings, 801, 600),
        Err(Error::BadRequest(_))
    ));
    assert!(matches!(
        check_dimensions(&settings, 800, 601),
        Err(Error::BadRequest(_))
    ));
    settings.max_width_pixels = 0; // 0 = unlimited, per axis
    assert!(check_dimensions(&settings, u32::MAX, 600).is_ok());
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rejects_file_over_size_limit(pool: PgPool) {
    let mut settings = test_settings();
    settings.max_file_size_bytes = 5; // the png is 33 bytes
    assert_eq!(
        upload_status(pool, settings, tiny_png(2, 2)).await,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rejects_image_over_width_limit(pool: PgPool) {
    // Width over, height unlimited: isolates the width check.
    let mut settings = test_settings();
    settings.max_width_pixels = 1; // the image is 2 wide
    assert_eq!(
        upload_status(pool, settings, tiny_png(2, 1)).await,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rejects_image_over_height_limit(pool: PgPool) {
    // Height over, width unlimited: isolates the height check.
    let mut settings = test_settings();
    settings.max_height_pixels = 1; // the image is 2 tall
    assert_eq!(
        upload_status(pool, settings, tiny_png(1, 2)).await,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_within_limits_succeeds(pool: PgPool) {
    // Under both limits: validation passes, the row inserts, and the file is
    // stored, so the endpoint returns 200.
    let mut settings = test_settings();
    settings.max_file_size_bytes = 1024;
    settings.max_width_pixels = 16;
    settings.max_height_pixels = 16;
    assert_eq!(
        upload_status(pool, settings, tiny_png(2, 2)).await,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn rehost_stores_a_fetched_image(pool: PgPool) {
    let (storage, dir) = temp_storage();
    let url = serve_once("HTTP/1.1 200 OK", "image/png", tiny_png(2, 2));
    let stored = store_from_url(&pool, &storage, &test_settings(), &url, &Default::default())
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
    let settings = test_settings();
    let image = tiny_png(2, 2);

    let first = store_from_url(
        &pool,
        &storage,
        &settings,
        &serve_once("HTTP/1.1 200 OK", "image/png", image.clone()),
        &Default::default(),
    )
    .await
    .unwrap();
    assert!(!first.existed);

    let second = store_from_url(
        &pool,
        &storage,
        &settings,
        &serve_once("HTTP/1.1 200 OK", "image/png", image.clone()),
        &Default::default(),
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
        store_from_url(&pool, &storage, &test_settings(), &url, &Default::default()).await,
        Err(Error::UnsupportedMediaType)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[sqlx::test(migrations = "./migrations")]
async fn rehost_rejects_a_failed_fetch(pool: PgPool) {
    let (storage, dir) = temp_storage();
    let url = serve_once("HTTP/1.1 404 Not Found", "text/plain", b"nope".to_vec());
    assert!(matches!(
        store_from_url(&pool, &storage, &test_settings(), &url, &Default::default()).await,
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

#[sqlx::test(migrations = "./migrations")]
async fn find_metadata_roundtrips_and_formats_created_at(pool: PgPool) {
    files::insert(&pool, &sample_file("meta1234"))
        .await
        .unwrap();

    let meta = files::find_metadata(&pool, "meta1234")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(meta.id, "meta1234");
    assert_eq!(meta.ext, "png");
    assert_eq!(meta.size, 33);
    assert_eq!((meta.width, meta.height), (2, 2));
    // created_at is rendered as a UTC ISO-8601 string (…T…Z), parseable by JS Date.
    assert!(meta.created_at.contains('T') && meta.created_at.ends_with('Z'));

    assert!(
        files::find_metadata(&pool, "missing0")
            .await
            .unwrap()
            .is_none()
    );
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
        original_hash: id,
        original_ext: "png",
        original_width: 2,
        original_height: 2,
        requested_quality: None,
        applied_quality: None,
        has_thumbnail: false,
    }
}
