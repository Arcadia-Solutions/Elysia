//! Tests for medium generation: config validation, when a medium is made
//! versus when /m/ falls back to the main file, and the stored medium format.

use crate::common;

use sqlx::PgPool;

use elysia::repository::files;
use elysia::services::files::store_from_url;

use common::*;

fn medium_settings(box_pixels: u32) -> elysia::settings::ElysiaSettings {
    let mut settings = test_settings();
    settings.max_width_pixels = 10_000;
    settings.max_height_pixels = 10_000;
    settings.medium_width_pixels = box_pixels;
    settings.medium_height_pixels = box_pixels;
    settings.medium_quality = 80;
    settings
}

#[test]
fn validate_requires_caps_and_quality_when_medium_enabled() {
    let mut settings = test_settings();
    settings.max_file_size_bytes = 50 * 1024 * 1024;
    settings.medium_width_pixels = 100; // pixel caps still 0 (unbounded decode)
    assert!(settings.validate().is_err());

    settings.max_width_pixels = 1000;
    settings.max_height_pixels = 1000;
    assert!(settings.validate().is_ok());

    settings.medium_quality = 0; // out of the 1..=100 range
    assert!(settings.validate().is_err());
}

#[sqlx::test(migrations = "./migrations")]
async fn medium_generated_as_webp_when_image_exceeds_box(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let settings = medium_settings(4);
    let url = serve_once("HTTP/1.1 200 OK", "image/png", real_png(16, 16));
    let uploader = test_user(&pool, "resizer").await.0.id;
    let stored = store_from_url(
        &pool,
        &storage,
        &settings,
        &reqwest::Client::new(),
        &url,
        None,
        &Default::default(),
        uploader,
        None,
    )
    .await
    .unwrap();

    let row = files::find(&pool, &stored.id).await.unwrap().unwrap();
    assert!(row.has_medium);
    let medium = storage.medium_path_for(&stored.id);
    let bytes = std::fs::read(&medium).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF", "medium is a WebP");
}

#[sqlx::test(migrations = "./migrations")]
async fn no_medium_when_image_fits_box(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let settings = medium_settings(64);
    let url = serve_once("HTTP/1.1 200 OK", "image/png", real_png(16, 16));
    let uploader = test_user(&pool, "resizer").await.0.id;
    let stored = store_from_url(
        &pool,
        &storage,
        &settings,
        &reqwest::Client::new(),
        &url,
        None,
        &Default::default(),
        uploader,
        None,
    )
    .await
    .unwrap();

    let row = files::find(&pool, &stored.id).await.unwrap().unwrap();
    assert!(!row.has_medium);
    assert!(!storage.medium_path_for(&stored.id).exists());
}
