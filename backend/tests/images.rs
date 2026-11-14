//! Browse pagination: page size, total count, the empty tail page and sorting.

use crate::common;

use std::sync::RwLock;

use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::{App, web::Data};
use sqlx::PgPool;

use common::page;
use elysia::handlers::images::parse_filename;
use elysia::repository::checksums as checksum_repository;
use elysia::repository::files::{self, ImagesQuery, ImagesSortBy, SortOrder, ViewKind};
use elysia::routes;
use elysia::services::checksums::{self, ChecksumAlgorithm, ComputeChecksumsParams};
use elysia::services::files::list_images;
use elysia::services::views::ViewCounter;
use elysia::settings::ElysiaSettings;
use elysia::storage::Preview;

#[sqlx::test]
async fn paginates_with_total(pool: PgPool) {
    let per_page = page(1).page_size as usize;
    let uploader = common::test_user(&pool, "browser").await.0.id;
    common::seed_files(&pool, uploader, "id", per_page + 1, 1).await;

    let (first, total) = list_images(&pool, &page(1), None).await.unwrap();
    assert_eq!(first.len(), per_page);
    assert_eq!(total, per_page as i64 + 1);
    assert!(first.iter().all(|row| row.total == total));

    let (second, _) = list_images(&pool, &page(2), None).await.unwrap();
    assert_eq!(second.len(), 1);

    // Past the last page: no rows, but the total still reflects every file.
    let (empty, empty_total) = list_images(&pool, &page(3), None).await.unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty_total, per_page as i64 + 1);
}

#[sqlx::test]
async fn sorts_by_column_and_direction(pool: PgPool) {
    let uploader = common::test_user(&pool, "sorter").await.0.id;
    common::seed_files(&pool, uploader, "big", 1, 5).await;
    common::seed_files(&pool, uploader, "small", 1, 1).await;

    let first_id = |sort_by, order| {
        let pool = pool.clone();
        async move {
            list_images(
                &pool,
                &ImagesQuery {
                    sort_by,
                    order,
                    ..page(1)
                },
                None,
            )
            .await
            .unwrap()
            .0[0]
                .id
                .clone()
        }
    };
    assert_eq!(
        first_id(ImagesSortBy::Size, SortOrder::Asc).await,
        "small000000"
    );
    assert_eq!(
        first_id(ImagesSortBy::Size, SortOrder::Desc).await,
        "big000000"
    );
    // seed_files uses the id as the original hash.
    assert_eq!(
        first_id(ImagesSortBy::OriginalHash, SortOrder::Asc).await,
        "big000000"
    );
    assert_eq!(
        first_id(ImagesSortBy::CreatedAt, SortOrder::Desc).await,
        "small000000"
    );
}

#[sqlx::test]
async fn counts_image_and_preview_views(pool: PgPool) {
    let uploader = common::test_user(&pool, "viewer").await.0.id;
    common::seed_files(&pool, uploader, "seen", 1, 1).await;
    common::seed_files(&pool, uploader, "unseen", 1, 1).await;
    let id = "seen000000";

    let views = ViewCounter::default();
    for (referrer, preview) in [
        ("a.com", None),
        ("a.com", None),
        ("", Some(Preview::Medium)),
        ("b.com", None),
        ("b.com", Some(Preview::Thumbnail)),
    ] {
        views.count(id, referrer, preview);
    }
    views.count("missing0", "a.com", None);
    views.flush(&pool).await;
    // Flushed counts are cleared, so a second flush adds nothing.
    views.flush(&pool).await;

    let metadata = files::find_metadata(&pool, id).await.unwrap().unwrap();
    assert_eq!(
        (
            metadata.views,
            metadata.medium_views,
            metadata.thumbnail_views
        ),
        (3, 1, 1)
    );
    // A second batch adds onto the existing referrer rows.
    views.count(id, "b.com", None);
    views.flush(&pool).await;
    let referrers: Vec<_> = files::find_referrer_views(&pool, id)
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            (
                row.referrer,
                row.views,
                row.medium_views,
                row.thumbnail_views,
            )
        })
        .collect();
    assert_eq!(
        referrers,
        [
            ("b.com".into(), 2, 0, 1),
            ("a.com".into(), 2, 0, 0),
            (String::new(), 0, 1, 0)
        ]
    );

    for sort_by in [
        ImagesSortBy::Views,
        ImagesSortBy::MediumViews,
        ImagesSortBy::ThumbnailViews,
    ] {
        let rows = list_images(
            &pool,
            &ImagesQuery {
                sort_by,
                order: SortOrder::Desc,
                ..page(1)
            },
            None,
        )
        .await
        .unwrap()
        .0;
        assert_eq!(rows[0].id, id);
    }
}

#[test]
fn parses_only_plain_filenames() {
    assert_eq!(
        parse_filename("abc12345.png").ok(),
        Some(("abc12345", "png"))
    );
    for filename in [
        "abc12345",
        "a.png",
        ".png",
        "abc12345.",
        "abc12345.m.webp",
        "../etc.png",
        "ab/cd.png",
        ".tmpAbC.png",
    ] {
        assert!(parse_filename(filename).is_err(), "{filename}");
    }
}

#[sqlx::test]
async fn filters_by_uploader(pool: PgPool) {
    let mine = common::test_user(&pool, "mine").await.0.id;
    let other = common::test_user(&pool, "other").await.0.id;
    common::seed_files(&pool, mine, "mine", 2, 1).await;
    common::seed_files(&pool, other, "other", 3, 1).await;

    let (rows, total) = list_images(&pool, &page(1), Some(mine)).await.unwrap();
    assert_eq!(total, 2);
    assert!(rows.iter().all(|row| row.id.starts_with("mine")));
    assert_eq!(list_images(&pool, &page(1), None).await.unwrap().1, 5);
}

#[sqlx::test]
async fn resets_one_view_counter(pool: PgPool) {
    let uploader = common::test_user(&pool, "viewer").await.0.id;
    common::seed_files(&pool, uploader, "seen", 2, 1).await;
    let views = ViewCounter::default();
    for id in ["seen000000", "seen000001"] {
        views.count(id, "a.com", None);
        views.count(id, "b.com", None);
        views.count(id, "b.com", Some(Preview::Thumbnail));
    }
    views.flush(&pool).await;
    let counters = |id: &'static str| {
        let pool = pool.clone();
        async move {
            let metadata = files::find_metadata(&pool, id).await.unwrap().unwrap();
            let referrers: Vec<_> = files::find_referrer_views(&pool, id)
                .await
                .unwrap()
                .into_iter()
                .map(|row| (row.referrer, row.views))
                .collect();
            (metadata.views, metadata.thumbnail_views, referrers)
        }
    };

    // One image and referrer: the total loses a.com's view, its emptied row is dropped.
    files::reset_views(
        &pool,
        Some("seen000000"),
        ViewKind::Image,
        &["a.com".into()],
    )
    .await
    .unwrap();
    assert_eq!(
        counters("seen000000").await,
        (1, 1, vec![("b.com".into(), 1)])
    );
    assert_eq!(counters("seen000001").await.0, 2);

    // Every image and referrer.
    files::reset_views(&pool, None, ViewKind::Thumbnail, &[])
        .await
        .unwrap();
    assert_eq!(
        counters("seen000001").await,
        (2, 0, vec![("a.com".into(), 1), ("b.com".into(), 1)])
    );

    // Every counter of one referrer.
    files::reset_views(&pool, None, ViewKind::All, &["b.com".into()])
        .await
        .unwrap();
    assert_eq!(
        counters("seen000001").await,
        (1, 0, vec![("a.com".into(), 1)])
    );
}

#[sqlx::test(fixtures("checksums"))]
async fn computes_and_serves_checksums(pool: PgPool) {
    let (storage, _dir) = common::temp_storage();
    let path = storage.path_for("hashed00", "png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"bytes").unwrap();
    let sha512 = checksums::hash_file(&path, &[ChecksumAlgorithm::Sha512])
        .await
        .unwrap()
        .remove(0);
    let coverage = |pool: PgPool| async move {
        let counts = checksums::counts(&pool).await.unwrap();
        let files: Vec<_> = counts
            .counts
            .iter()
            .map(|count| (count.algorithm, count.files))
            .collect();
        (counts.total_files, files)
    };
    assert_eq!(
        coverage(pool.clone()).await,
        (1, vec![(ChecksumAlgorithm::Sha256, 1)])
    );

    let storage = Data::new(storage);
    let params = ComputeChecksumsParams {
        algorithms: vec![ChecksumAlgorithm::Sha512],
    };
    checksums::start_compute(pool.clone(), storage.clone(), params).unwrap();
    while checksums::progress().running {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let progress = checksums::progress();
    assert_eq!(
        (progress.done, progress.total, progress.error),
        (1, 1, None)
    );
    assert_eq!(
        coverage(pool.clone()).await,
        (
            1,
            vec![
                (ChecksumAlgorithm::Sha256, 1),
                (ChecksumAlgorithm::Sha512, 1)
            ]
        )
    );

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(common::test_config()))
            .app_data(Data::new(pool))
            .app_data(storage)
            .app_data(Data::new(RwLock::new(common::test_settings())))
            .app_data(Data::new(ViewCounter::default()))
            .configure(routes::init),
    )
    .await;
    let sha256 = "ab".repeat(32);
    // The extension is optional and ignored, the digest case-insensitive.
    // Without generated previews, /mh/ and /th/ serve the image.
    for uri in [
        format!("/ih/sha256/{sha256}"),
        format!("/ih/sha256/{}.jpg", sha256.to_uppercase()),
        format!("/ih/sha512/{sha512}.png"),
        format!("/mh/sha256/{sha256}.png"),
        format!("/th/sha512/{sha512}.png"),
    ] {
        let response =
            actix_test::call_service(&app, actix_test::TestRequest::get().uri(&uri).to_request())
                .await;
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        assert_eq!(actix_test::read_body(response).await, "bytes");
    }
    for uri in [
        format!("/ih/sha256/{}.png", "cd".repeat(32)),
        format!("/ih/sha512/{sha256}.png"),
        format!("/ih/md5/{sha256}.png"),
        "/ih/sha256/hashed00.png".to_string(),
    ] {
        let request = actix_test::TestRequest::get().uri(&uri).to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::NOT_FOUND,
            "{uri}"
        );
    }
}

#[sqlx::test]
async fn upload_records_enabled_checksums(pool: PgPool) {
    let (_, token) = common::test_user(&pool, "hasher").await;
    let settings = ElysiaSettings {
        checksum_algorithms: vec![ChecksumAlgorithm::Md5],
        ..common::test_settings()
    };
    let (status, body) =
        common::upload_with_token(pool.clone(), settings, common::real_png(2, 2), "", &token).await;
    assert_eq!(status, StatusCode::OK);
    let id = body["id"].as_str().unwrap();
    let recorded: Vec<_> = checksum_repository::for_file(&pool, id)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.algorithm, row.digest.len()))
        .collect();
    assert_eq!(
        recorded,
        [
            (ChecksumAlgorithm::Md5, 32),
            (ChecksumAlgorithm::Sha256, 64)
        ]
    );
}

#[tokio::test]
async fn hashes_with_every_algorithm() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let digests = checksums::hash_file(file.path(), ChecksumAlgorithm::ALL)
        .await
        .unwrap();
    assert!(digests.iter().all(|digest| !digest.is_empty()));
    let digest_of = |algorithm| {
        let index = ChecksumAlgorithm::ALL
            .iter()
            .position(|other| *other == algorithm)
            .unwrap();
        digests[index].as_str()
    };
    assert_eq!(
        digest_of(ChecksumAlgorithm::Md5),
        "d41d8cd98f00b204e9800998ecf8427e"
    );
    assert_eq!(
        digest_of(ChecksumAlgorithm::Sha1),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
    assert_eq!(
        digest_of(ChecksumAlgorithm::Sha256),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn serializes_algorithms_by_name() {
    for algorithm in ChecksumAlgorithm::ALL {
        assert_eq!(
            serde_json::to_value(algorithm).unwrap(),
            algorithm.name(),
            "{algorithm:?}"
        );
    }
}
