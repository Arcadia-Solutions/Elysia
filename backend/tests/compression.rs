//! Tests for the image-processing feature: the pure pipeline, the processing
//! store path, config validation for processing, and the quality query option.

mod common;

use actix_web::http::StatusCode;
use sqlx::PgPool;

use elysia::handlers::images::upload::UploadOptionsQuery;
use elysia::services::files::{UploadOptions, store_from_url};
use elysia::services::image::{ProcessError, ProcessOptions, TargetFormat, process};
use elysia::settings::ElysiaSettings;

use common::*;

#[test]
fn validate_rejects_processing_without_target_format() {
    let mut settings = test_settings();
    settings.default_target_file_format = None;
    settings.target_width_pixels = 1920;
    assert!(settings.validate().is_err());
}

#[test]
fn validate_requires_max_file_size() {
    // Unconditional: an upload with no byte ceiling is rejected even with the
    // processing feature off.
    let mut settings = test_settings();
    assert!(settings.validate().is_err());
    settings.max_file_size_bytes = 50 * 1024 * 1024;
    assert!(settings.validate().is_ok());
}

#[test]
fn validate_rejects_processing_without_decode_caps() {
    let mut settings = test_settings();
    settings.max_file_size_bytes = 50 * 1024 * 1024;
    settings.default_target_file_format = Some(TargetFormat::Webp);
    // pixel caps left at 0 (unlimited): processing would decode unbounded input.
    assert!(settings.validate().is_err());
}

#[test]
fn validate_accepts_consistent_settings() {
    let mut settings = test_settings();
    settings.default_target_file_format = Some(TargetFormat::Webp);
    settings.target_width_pixels = 1920;
    settings.max_file_size_bytes = 50 * 1024 * 1024;
    settings.max_width_pixels = 10_000;
    settings.max_height_pixels = 10_000;
    assert!(settings.validate().is_ok());
    // feature off is valid once the byte ceiling is set
    let mut off = test_settings();
    off.max_file_size_bytes = 50 * 1024 * 1024;
    assert!(off.validate().is_ok());
}

#[test]
fn validate_rejects_default_compression_without_format() {
    let mut settings = test_settings();
    settings.max_file_size_bytes = 50 * 1024 * 1024;
    settings.default_compression = 50; // no target format to re-encode to
    assert!(settings.validate().is_err());
}

#[test]
fn validate_lossless_incapable_format_needs_quality_source() {
    // Base: valid AVIF (no lossless mode) processing config.
    let mut settings = test_settings();
    settings.default_target_file_format = Some(TargetFormat::Avif);
    settings.max_file_size_bytes = 50 * 1024 * 1024;
    settings.max_width_pixels = 10_000;
    settings.max_height_pixels = 10_000;

    // No default compression and no override: no quality source, rejected.
    settings.default_compression = 0;
    settings.allow_overriding_compression = false;
    assert!(settings.validate().is_err());

    // A default compression supplies the quality.
    settings.default_compression = 80;
    assert!(settings.validate().is_ok());

    // Or allowing per-upload override does.
    settings.default_compression = 0;
    settings.allow_overriding_compression = true;
    assert!(settings.validate().is_ok());

    // A lossless-capable default (WebP) is fine with neither.
    settings.default_target_file_format = Some(TargetFormat::Webp);
    settings.allow_overriding_compression = false;
    assert!(settings.validate().is_ok());
}

#[test]
fn default_compression_applies_when_none_requested() {
    let mut settings = webp_settings();
    settings.default_compression = 50;
    let options = UploadOptionsQuery {
        lossy_compression_value: None,
        target_file_format: None,
        strip_exif: None,
    }
    .into_options(&settings)
    .unwrap();
    assert_eq!(options.lossy_compression_value, Some(50));
}

#[test]
fn zero_requests_lossless_over_default() {
    let mut settings = webp_settings();
    settings.default_compression = 50;
    let options = UploadOptionsQuery {
        lossy_compression_value: Some(0),
        target_file_format: None,
        strip_exif: None,
    }
    .into_options(&settings)
    .unwrap();
    assert_eq!(options.lossy_compression_value, None);
}

#[test]
fn request_overrides_default_when_allowed() {
    let mut settings = webp_settings();
    settings.default_compression = 50;
    let options = UploadOptionsQuery {
        lossy_compression_value: Some(90),
        target_file_format: None,
        strip_exif: None,
    }
    .into_options(&settings)
    .unwrap();
    assert_eq!(options.lossy_compression_value, Some(90));
}

#[test]
fn override_rejected_when_disabled_but_default_still_used() {
    let mut settings = webp_settings();
    settings.allow_overriding_compression = false;
    settings.default_compression = 40;
    assert!(
        UploadOptionsQuery {
            lossy_compression_value: Some(90),
            target_file_format: None,
            strip_exif: None,
        }
        .into_options(&settings)
        .is_err()
    );
    let options = UploadOptionsQuery {
        lossy_compression_value: None,
        target_file_format: None,
        strip_exif: None,
    }
    .into_options(&settings)
    .unwrap();
    assert_eq!(options.lossy_compression_value, Some(40));
}

#[test]
fn file_format_override_respects_allow_flag() {
    let mut settings = webp_settings();

    // Forbidden by default: a requested format is rejected.
    assert!(
        UploadOptionsQuery {
            lossy_compression_value: None,
            target_file_format: Some(TargetFormat::Png),
            strip_exif: None,
        }
        .into_options(&settings)
        .is_err()
    );

    // Allowed: the request wins over the default format.
    settings.allow_overriding_file_format = true;
    let options = UploadOptionsQuery {
        lossy_compression_value: None,
        target_file_format: Some(TargetFormat::Png),
        strip_exif: None,
    }
    .into_options(&settings)
    .unwrap();
    assert_eq!(options.target_file_format, Some(TargetFormat::Png));
}

#[test]
fn strip_exif_override_respects_allow_flag() {
    let mut settings = webp_settings();
    settings.strip_exif_by_default = true;

    // Allowed by default: a request wins over the configured default.
    let options = UploadOptionsQuery {
        lossy_compression_value: None,
        target_file_format: None,
        strip_exif: Some(false),
    }
    .into_options(&settings)
    .unwrap();
    assert!(!options.strip_exif);

    // Forbidden: a request is rejected, and the default still applies when absent.
    settings.allow_overriding_strip_exif = false;
    assert!(
        UploadOptionsQuery {
            lossy_compression_value: None,
            target_file_format: None,
            strip_exif: Some(false),
        }
        .into_options(&settings)
        .is_err()
    );
    let options = UploadOptionsQuery {
        lossy_compression_value: None,
        target_file_format: None,
        strip_exif: None,
    }
    .into_options(&settings)
    .unwrap();
    assert!(options.strip_exif);
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_converts_to_webp_and_records_actions(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let url = serve_once("HTTP/1.1 200 OK", "image/png", real_png(8, 8));
    let uploader = test_user(&pool, "converter").await.0.id;
    let stored = store_from_url(
        &pool,
        &storage,
        &webp_settings(),
        &reqwest::Client::new(),
        &url,
        None,
        &Default::default(),
        uploader,
        None,
    )
    .await
    .unwrap();
    assert_eq!(stored.ext, "webp");
    let convert = stored.actions.unwrap().convert.unwrap();
    assert_eq!(
        (convert.from.as_str(), convert.to.as_str()),
        ("png", "webp")
    );
    assert!(storage.path_for(&stored.id, "webp").exists());
}

#[sqlx::test(migrations = "./migrations")]
async fn same_source_and_quality_skips_reprocessing(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let image = real_png(8, 8);
    let uploader = test_user(&pool, "skipper").await.0.id;
    let first = store_from_url(
        &pool,
        &storage,
        &webp_settings(),
        &reqwest::Client::new(),
        &serve_once("HTTP/1.1 200 OK", "image/png", image.clone()),
        None,
        &Default::default(),
        uploader,
        None,
    )
    .await
    .unwrap();
    assert!(!first.existed);
    let second = store_from_url(
        &pool,
        &storage,
        &webp_settings(),
        &reqwest::Client::new(),
        &serve_once("HTTP/1.1 200 OK", "image/png", image.clone()),
        None,
        &Default::default(),
        uploader,
        None,
    )
    .await
    .unwrap();
    assert!(second.existed);
    assert_eq!(first.id, second.id);
}

#[sqlx::test(migrations = "./migrations")]
async fn different_quality_reprocesses(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let image = noisy_png(32, 32);
    let uploader = test_user(&pool, "qualitychecker").await.0.id;
    let mut ids = Vec::new();
    for quality in [80, 30] {
        let options = UploadOptions {
            lossy_compression_value: Some(quality),
            target_file_format: Some(TargetFormat::Webp),
            strip_exif: false,
            ..Default::default()
        };
        let stored = store_from_url(
            &pool,
            &storage,
            &webp_settings(),
            &reqwest::Client::new(),
            &serve_once("HTTP/1.1 200 OK", "image/png", image.clone()),
            None,
            &options,
            uploader,
            None,
        )
        .await
        .unwrap();
        assert!(!stored.existed);
        ids.push(stored.id);
    }
    assert_ne!(ids[0], ids[1]);
}

#[sqlx::test(migrations = "./migrations")]
async fn feature_off_stores_as_is_with_no_actions(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let url = serve_once("HTTP/1.1 200 OK", "image/png", real_png(8, 8));
    let uploader = test_user(&pool, "passthrough").await.0.id;
    let stored = store_from_url(
        &pool,
        &storage,
        &test_settings(),
        &reqwest::Client::new(),
        &url,
        None,
        &Default::default(),
        uploader,
        None,
    )
    .await
    .unwrap();
    assert_eq!(stored.ext, "png");
    assert!(stored.actions.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rejects_compression_when_feature_off(pool: PgPool) {
    let status = upload_status_with_query(
        pool,
        test_settings(),
        real_png(8, 8),
        "lossy_compression_value=80",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// A real baseline JPEG carrying an EXIF APP1 segment, with an Orientation tag
/// when `orientation` is set. `color` varies the pixels so two fixtures hash
/// differently (the store dedups identical sources).
fn jpeg_with_exif(color: [u8; 3], width: u32, height: u32, orientation: Option<u8>) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb(color));
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    let jpeg = jpeg.into_inner();

    // Minimal EXIF: little-endian TIFF header + an IFD0 holding zero or one entry.
    let mut tiff = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]; // "II", 42, IFD0 at offset 8
    match orientation {
        // One entry: tag 0x0112 (Orientation), type SHORT, count 1, value.
        Some(value) => tiff.extend_from_slice(&[
            0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, value, 0x00, 0x00, 0x00,
        ]),
        None => tiff.extend_from_slice(&[0x00, 0x00]),
    }
    tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // next IFD = 0
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&tiff);
    let segment_len = (payload.len() + 2) as u16; // length field counts itself

    let mut app1 = vec![0xFF, 0xE1];
    app1.extend_from_slice(&segment_len.to_be_bytes());
    app1.extend_from_slice(&payload);

    // APP1 EXIF goes right after the SOI marker (the first two bytes).
    let mut out = Vec::with_capacity(jpeg.len() + app1.len());
    out.extend_from_slice(&jpeg[..2]);
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[sqlx::test(migrations = "./migrations")]
async fn strip_exif_removes_metadata_on_passthrough(pool: PgPool) {
    let (storage, _dir) = temp_storage();
    let exif_marker = &b"Exif\0\0"[..];
    // Processing off (store as-is), so stripping is the only thing touching bytes.
    let settings = test_settings();
    let uploader = test_user(&pool, "exifstripper").await.0.id;

    // Stripping on: the stored file must have no EXIF.
    let source = jpeg_with_exif([10, 20, 30], 8, 8, None);
    assert!(contains(&source, exif_marker), "fixture should carry EXIF");
    let stored = store_from_url(
        &pool,
        &storage,
        &settings,
        &reqwest::Client::new(),
        &serve_once("HTTP/1.1 200 OK", "image/jpeg", source),
        None,
        &UploadOptions {
            lossy_compression_value: None,
            target_file_format: None,
            strip_exif: true,
            ..Default::default()
        },
        uploader,
        None,
    )
    .await
    .unwrap();
    assert_eq!(stored.ext, "jpg");
    let stored_bytes = std::fs::read(storage.path_for(&stored.id, &stored.ext)).unwrap();
    assert!(
        !contains(&stored_bytes, exif_marker),
        "EXIF must be gone after stripping"
    );

    // Stripping off (a different source to dodge dedup): EXIF survives passthrough.
    let kept_source = jpeg_with_exif([200, 100, 50], 8, 8, None);
    let kept = store_from_url(
        &pool,
        &storage,
        &settings,
        &reqwest::Client::new(),
        &serve_once("HTTP/1.1 200 OK", "image/jpeg", kept_source),
        None,
        &UploadOptions {
            lossy_compression_value: None,
            target_file_format: None,
            strip_exif: false,
            ..Default::default()
        },
        uploader,
        None,
    )
    .await
    .unwrap();
    let kept_bytes = std::fs::read(storage.path_for(&kept.id, &kept.ext)).unwrap();
    assert!(
        contains(&kept_bytes, exif_marker),
        "passthrough without stripping keeps EXIF"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn exif_orientation_is_applied(pool: PgPool) {
    use image::ImageDecoder;
    let (storage, _dir) = temp_storage();
    let uploader = test_user(&pool, "orienter").await.0.id;
    // 16x8 pixels tagged Orientation=6 (rotate 90): displayed as 8x16.
    let cases = [
        (
            webp_settings(),
            [10, 20, 30],
            image::metadata::Orientation::NoTransforms,
        ),
        (
            test_settings(),
            [200, 100, 50],
            image::metadata::Orientation::Rotate90,
        ),
    ];
    for (settings, color, stored_orientation) in cases {
        let stored = store_from_url(
            &pool,
            &storage,
            &settings,
            &reqwest::Client::new(),
            &serve_once(
                "HTTP/1.1 200 OK",
                "image/jpeg",
                jpeg_with_exif(color, 16, 8, Some(6)),
            ),
            None,
            &UploadOptions {
                lossy_compression_value: None,
                target_file_format: None,
                strip_exif: true,
                ..Default::default()
            },
            uploader,
            None,
        )
        .await
        .unwrap();
        assert!(
            stored
                .actions
                .is_none_or(|actions| actions.resize.is_none())
        );
        let row = elysia::repository::files::find_metadata(&pool, &stored.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!((row.width, row.height), (8, 16));
        assert_eq!((row.original_width, row.original_height), (8, 16));
        // The pipeline rotates the pixels; a stripped passthrough keeps the tag.
        let bytes = std::fs::read(storage.path_for(&stored.id, &stored.ext)).unwrap();
        let mut decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        assert_eq!(decoder.orientation().unwrap(), stored_orientation);
        let (width, height) = decoder.dimensions();
        let upright = if stored_orientation == image::metadata::Orientation::Rotate90 {
            (height, width)
        } else {
            (width, height)
        };
        assert_eq!(upright, (8, 16));
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rejects_quality_above_100(pool: PgPool) {
    let status = upload_status_with_query(
        pool,
        webp_settings(),
        real_png(8, 8),
        "lossy_compression_value=150",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_accepts_quality_zero_as_lossless(pool: PgPool) {
    let status = upload_status_with_query(
        pool,
        webp_settings(),
        real_png(8, 8),
        "lossy_compression_value=0",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_accepts_compression_when_feature_on(pool: PgPool) {
    let status = upload_status_with_query(
        pool,
        webp_settings(),
        real_png(8, 8),
        "lossy_compression_value=80",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[test]
fn pipeline_converts_png_to_webp() {
    let out = process(&real_png(8, 8), &pipeline_options(TargetFormat::Webp))
        .unwrap()
        .unwrap();
    assert_eq!(out.ext, "webp");
    assert_eq!((out.width, out.height), (8, 8));
    assert_eq!(&out.bytes[0..4], b"RIFF");
}

#[test]
fn pipeline_keeps_icc_profile() {
    use image::{ImageDecoder, ImageEncoder};
    let icc_profile = b"stand-in for a Display P3 profile".to_vec();
    let mut source = Vec::new();
    let mut encoder = image::codecs::png::PngEncoder::new(&mut source);
    encoder.set_icc_profile(icc_profile.clone()).unwrap();
    encoder
        .write_image(&[0; 4 * 8 * 8], 8, 8, image::ExtendedColorType::Rgba8)
        .unwrap();

    for (format, quality) in [
        (TargetFormat::Webp, None),
        (TargetFormat::Png, None),
        (TargetFormat::Jpg, Some(80)),
    ] {
        let options = ProcessOptions {
            requested_quality: quality,
            ..pipeline_options(format)
        };
        let out = process(&source, &options).unwrap().unwrap();
        let mut decoder = image::ImageReader::new(std::io::Cursor::new(out.bytes))
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        assert_eq!(decoder.icc_profile().unwrap(), Some(icc_profile.clone()));
    }
}

#[test]
fn pipeline_resizes_within_the_box_preserving_aspect() {
    let mut options = pipeline_options(TargetFormat::Webp);
    options.target_width = 4;
    options.target_height = 4;
    let out = process(&real_png(10, 5), &options).unwrap().unwrap();
    assert_eq!((out.width, out.height), (4, 2));
}

#[test]
fn pipeline_does_not_upscale() {
    let mut options = pipeline_options(TargetFormat::Webp);
    options.target_width = 100;
    options.target_height = 100;
    let out = process(&real_png(8, 8), &options).unwrap().unwrap();
    assert_eq!((out.width, out.height), (8, 8));
}

#[test]
fn pipeline_lossy_is_smaller_than_lossless() {
    let img = noisy_png(64, 64);
    let lossless = process(&img, &pipeline_options(TargetFormat::Webp))
        .unwrap()
        .unwrap();
    let mut options = pipeline_options(TargetFormat::Webp);
    options.requested_quality = Some(10);
    let lossy = process(&img, &options).unwrap().unwrap();
    assert_eq!(lossy.applied_quality, Some(10));
    assert!(lossy.bytes.len() < lossless.bytes.len());
}

#[test]
fn pipeline_rejects_undecodable_bytes() {
    assert!(matches!(
        process(b"\x89PNG\r\n\x1a\n", &pipeline_options(TargetFormat::Webp)),
        Err(ProcessError::Undecodable)
    ));
}

#[test]
fn pipeline_passes_through_animated_gif() {
    assert!(
        process(&animated_gif(), &pipeline_options(TargetFormat::Webp))
            .unwrap()
            .is_none()
    );
}

#[test]
fn pipeline_encodes_avif() {
    let mut options = pipeline_options(TargetFormat::Avif);
    options.requested_quality = Some(80);
    let out = process(&real_png(16, 16), &options).unwrap().unwrap();
    assert_eq!(out.ext, "avif");
    assert_eq!((out.width, out.height), (16, 16));
    assert_eq!(&out.bytes[4..8], b"ftyp");
}

#[test]
fn pipeline_avif_lossless_is_rejected() {
    // No quality + AVIF would be near-lossless, so it must error out.
    let result = process(&real_png(16, 16), &pipeline_options(TargetFormat::Avif));
    assert!(matches!(result, Err(ProcessError::Encode(_))));
}

#[test]
/// PNG has no lossy mode, so a quality request must error rather than
/// silently encode lossless.
fn pipeline_png_lossy_is_rejected() {
    let mut options = pipeline_options(TargetFormat::Png);
    options.requested_quality = Some(80);
    let result = process(&real_png(16, 16), &options);
    assert!(matches!(result, Err(ProcessError::Encode(_))));
}

#[test]
fn pipeline_encodes_jxl() {
    let out = process(&real_png(16, 16), &pipeline_options(TargetFormat::Jpegxl))
        .unwrap()
        .unwrap();
    assert_eq!(out.ext, "jxl");
    assert_eq!((out.width, out.height), (16, 16));
    assert!(!out.bytes.is_empty());
}

#[test]
fn pipeline_jxl_lossy() {
    let mut options = pipeline_options(TargetFormat::Jpegxl);
    options.requested_quality = Some(40);
    let out = process(&real_png(16, 16), &options).unwrap().unwrap();
    assert_eq!(out.ext, "jxl");
    assert_eq!(out.applied_quality, Some(40));
    assert_eq!((out.width, out.height), (16, 16));
    assert!(!out.bytes.is_empty());
}

#[test]
fn pipeline_jxl_quality_100() {
    let mut options = pipeline_options(TargetFormat::Jpegxl);
    options.requested_quality = Some(100);
    let out = process(&real_png(16, 16), &options).unwrap().unwrap();
    assert_eq!(out.ext, "jxl");
    assert_eq!(out.applied_quality, Some(100));
    assert!(!out.bytes.is_empty());
}

#[test]
fn pipeline_target_file_size_triggers_lossy() {
    let img = noisy_png(64, 64);
    let lossless = process(&img, &pipeline_options(TargetFormat::Webp))
        .unwrap()
        .unwrap();
    let mut options = pipeline_options(TargetFormat::Webp);
    options.target_file_size = lossless.bytes.len() as u64 / 2;
    let out = process(&img, &options).unwrap().unwrap();
    assert!(out.applied_quality.is_some());
    assert!(out.bytes.len() as u64 <= options.target_file_size);
}

#[test]
fn target_format_parses_names_and_rejects_unknown() {
    let parse = |s: &str| serde_json::from_str::<TargetFormat>(&format!("\"{s}\""));
    assert!(matches!(parse("jpegxl"), Ok(TargetFormat::Jpegxl)));
    assert!(matches!(parse("jxl"), Ok(TargetFormat::Jpegxl)));
    assert!(parse("bmp").is_err());
}

#[test]
fn settings_json_roundtrips_target_format() {
    // The PUT /api/elysia-settings body carries the format by name; the alias
    // `jxl` is accepted on input and canonicalizes to `jpegxl` on output.
    let parsed: ElysiaSettings =
        serde_json::from_str(r#"{"max_file_size_bytes":1,"default_target_file_format":"jxl"}"#)
            .unwrap();
    assert_eq!(
        parsed.default_target_file_format,
        Some(TargetFormat::Jpegxl)
    );
    let json = serde_json::to_string(&parsed).unwrap();
    assert!(json.contains("\"jpegxl\""));
}
