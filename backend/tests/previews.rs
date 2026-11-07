//! Tests for preview generation: config validation, when a medium and a
//! thumbnail are made versus when they fall back to the main file, and their format.

use crate::common;

use sqlx::PgPool;

use elysia::repository::files;
use elysia::services::files::store_from_url;
use elysia::storage::Preview;

use common::*;

fn preview_settings(box_pixels: u32) -> elysia::settings::ElysiaSettings {
    let mut settings = test_settings();
    settings.max_width_pixels = 10_000;
    settings.max_height_pixels = 10_000;
    settings.medium_width_pixels = box_pixels;
    settings.medium_height_pixels = box_pixels;
    settings.medium_quality = 80;
    settings.thumbnail_width_pixels = box_pixels;
    settings.thumbnail_height_pixels = box_pixels;
    settings.thumbnail_quality = 80;
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

    settings.medium_quality = 80;
    settings.thumbnail_width_pixels = 100;
    settings.thumbnail_height_pixels = 100;
    settings.thumbnail_quality = 0;
    assert!(settings.validate().is_err());
}

#[sqlx::test(migrations = "./migrations")]
async fn previews_generated_as_webp_when_image_exceeds_box(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let settings = preview_settings(4);
    let url = serve_once("HTTP/1.1 200 OK", "image/png", real_png(32, 16));
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
    assert!(stored.has_thumbnail);
    let medium = storage.preview_path_for(&stored.id, Preview::Medium);
    let bytes = std::fs::read(&medium).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF", "medium is a WebP");
    assert_eq!(image::image_dimensions(&medium).unwrap(), (4, 2));
    // The thumbnail is cropped to the box aspect ratio, so it fills the box.
    let thumbnail = storage.preview_path_for(&stored.id, Preview::Thumbnail);
    assert_eq!(image::image_dimensions(&thumbnail).unwrap(), (4, 4));
}

#[sqlx::test(migrations = "./migrations")]
async fn no_previews_when_image_fits_box(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let settings = preview_settings(64);
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
    for preview in Preview::ALL {
        assert!(!storage.preview_path_for(&stored.id, preview).exists());
    }
}
