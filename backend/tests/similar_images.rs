//! Finding images that look alike by their perceptual hash.

mod common;

use std::num::NonZeroU32;

use actix_web::http::StatusCode;
use image::{DynamicImage, ImageFormat, RgbImage};
use sqlx::PgPool;

use elysia::services::image::perceptual_hash_of;
use elysia::services::similar_images::{
    SimilarProgress, SimilarScanParams, group_similar, list_similar, progress, start_scan,
};

/// A disc on a gradient.
fn scene(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
        let (center_x, center_y) = (width as f32 * 0.35, height as f32 * 0.45);
        if (x as f32 - center_x).hypot(y as f32 - center_y) < width as f32 * 0.2 {
            image::Rgb([230, 40, 40])
        } else {
            image::Rgb([(x * 255 / width) as u8, (y * 255 / height) as u8, 90])
        }
    }))
}

/// Diagonal stripes, unrelated to `scene`.
fn stripes(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
        let shade = if ((x + 2 * y) * 6 / width).is_multiple_of(2) {
            20
        } else {
            220
        };
        image::Rgb([shade, shade, shade])
    }))
}

fn encode(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    image.write_to(&mut cursor, format).unwrap();
    cursor.into_inner()
}

fn distance(first: &[u8], second: &[u8]) -> u32 {
    (perceptual_hash_of(first).unwrap() ^ perceptual_hash_of(second).unwrap()).count_ones()
}

#[test]
fn perceptual_hash_survives_reencoding_and_resizing() {
    let original = encode(&scene(400, 300), ImageFormat::Png);
    let jpeg = encode(&scene(400, 300), ImageFormat::Jpeg);
    let smaller = encode(&scene(160, 120), ImageFormat::Png);
    let other = encode(&stripes(400, 300), ImageFormat::Png);
    assert!(distance(&original, &jpeg) <= 4);
    assert!(distance(&original, &smaller) <= 6);
    assert!(distance(&original, &other) > 16);
}

#[test]
fn groups_transitively_within_the_distance() {
    // 0 and 0b1 differ in 1 bit, 0b1 and 0b1111 in 3, 0 and 0b1111 in 4.
    let hashes = [0b1111, -1, 0, 0b1, 0x7777_0000];
    assert_eq!(
        group_similar(&hashes, 3, |_| Ok(())).unwrap(),
        vec![vec![0, 2, 3]]
    );
    assert_eq!(
        group_similar(&hashes, 0, |_| Ok(())).unwrap(),
        Vec::<Vec<usize>>::new()
    );
}

async fn finished() -> SimilarProgress {
    loop {
        let current = progress();
        if !current.running {
            return current;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Uploads are hashed whether converted or stored as is, and only look-alikes
/// are grouped, largest first.
#[sqlx::test(migrations = "./migrations")]
async fn scans_and_lists_similar_images(pool: PgPool) {
    let (_, token) = common::test_user(&pool, "uploader").await;
    for (settings, image) in [
        (
            common::test_settings(),
            encode(&scene(160, 120), ImageFormat::Png),
        ),
        (
            common::webp_settings(),
            encode(&scene(400, 300), ImageFormat::Jpeg),
        ),
        (
            common::test_settings(),
            encode(&stripes(400, 300), ImageFormat::Png),
        ),
    ] {
        let (status, _) =
            common::upload_with_token(pool.clone(), settings, image, "", &token).await;
        assert_eq!(status, StatusCode::OK);
    }

    start_scan(pool.clone(), SimilarScanParams { max_distance: 8 }).unwrap();
    let result = finished().await;
    assert_eq!(result.error, None);
    assert_eq!((result.done, result.total), (3, 3));
    assert!(result.scanned_at.is_some());

    let page = NonZeroU32::new(1).unwrap();
    let (groups, total) = list_similar(&pool, page, 50).await.unwrap();
    assert_eq!(total, 1);
    let sizes: Vec<(i32, i32)> = groups[0]
        .iter()
        .map(|row| (row.width, row.height))
        .collect();
    assert_eq!(sizes, vec![(400, 300), (160, 120)]);
}
