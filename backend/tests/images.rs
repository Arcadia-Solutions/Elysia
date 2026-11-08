//! Browse pagination: page size, total count, the empty tail page and sorting.

use crate::common;

use sqlx::PgPool;

use common::page;
use elysia::handlers::images::parse_filename;
use elysia::repository::files::{self, ImagesQuery, ImagesSortBy, SortOrder, ViewKind};
use elysia::services::files::list_images;
use elysia::services::views::ViewCounter;
use elysia::storage::Preview;

#[sqlx::test(migrations = "./migrations")]
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

#[sqlx::test(migrations = "./migrations")]
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

#[sqlx::test(migrations = "./migrations")]
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

#[sqlx::test(migrations = "./migrations")]
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

#[sqlx::test(migrations = "./migrations")]
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
