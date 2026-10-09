//! Tests for thumbnail generation: config validation, when a thumbnail is made
//! versus when /t/ falls back to the main file, and the stored thumbnail format.

mod common;

use sqlx::PgPool;

use elysia::repository::files;
use elysia::services::files::store_from_url;

use common::*;

fn thumbnail_settings(box_pixels: u32) -> elysia::settings::ElysiaSettings {
    let mut settings = test_settings();
    settings.max_width_pixels = 10_000;
    settings.max_height_pixels = 10_000;
    settings.thumbnail_width_pixels = box_pixels;
    settings.thumbnail_height_pixels = box_pixels;
    settings.thumbnail_quality = 80;
    settings
}

#[test]
fn validate_requires_caps_and_quality_when_thumbnails_enabled() {
    let mut settings = test_settings();
    settings.max_file_size_bytes = 50 * 1024 * 1024;
    settings.thumbnail_width_pixels = 100; // pixel caps still 0 (unbounded decode)
    assert!(settings.validate().is_err());

    settings.max_width_pixels = 1000;
    settings.max_height_pixels = 1000;
    assert!(settings.validate().is_ok());

    settings.thumbnail_quality = 0; // out of the 1..=100 range
    assert!(settings.validate().is_err());
}

#[sqlx::test(migrations = "./migrations")]
async fn thumbnail_generated_as_webp_when_image_exceeds_box(pool: PgPool) {
    let (storage, dir) = temp_storage();
    let settings = thumbnail_settings(4);
    let url = serve_once("HTTP/1.1 200 OK", "image/png", real_png(16, 16));
    let stored = store_from_url(&pool, &storage, &settings, &url, &Default::default())
        .await
        .unwrap();

    let row = files::find(&pool, &stored.id).await.unwrap().unwrap();
    assert!(row.has_thumbnail);
    let thumbnail = storage.thumbnail_path_for(&stored.id);
    let bytes = std::fs::read(&thumbnail).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF", "thumbnail is a WebP");
    let _ = std::fs::remove_dir_all(&dir);
}

#[sqlx::test(migrations = "./migrations")]
async fn no_thumbnail_when_image_fits_box(pool: PgPool) {
    let (storage, dir) = temp_storage();
    let settings = thumbnail_settings(64);
    let url = serve_once("HTTP/1.1 200 OK", "image/png", real_png(16, 16));
    let stored = store_from_url(&pool, &storage, &settings, &url, &Default::default())
        .await
        .unwrap();

    let row = files::find(&pool, &stored.id).await.unwrap().unwrap();
    assert!(!row.has_thumbnail);
    assert!(!storage.thumbnail_path_for(&stored.id).exists());
    let _ = std::fs::remove_dir_all(&dir);
}
