//! Browse pagination: page size, total count, the empty tail page and sorting.

mod common;

use sqlx::PgPool;

use common::page;
use elysia::handlers::images::parse_filename;
use elysia::repository::files::{self, ImagesQuery, ImagesSortBy, SortOrder};
use elysia::services::files::list_images;
use elysia::services::views::ViewCounter;

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
async fn backdate_only_moves_created_at_earlier(pool: PgPool) {
    let uploader = common::test_user(&pool, "importer").await.0.id;
    common::seed_files(&pool, uploader, "id", 1, 1).await;
    let id = "id000000";

    // 2020-01-01T00:00:00Z is before now, so the date moves.
    files::backdate_created_at(&pool, id, 1_577_836_800)
        .await
        .unwrap();
    let metadata = files::find_metadata(&pool, id).await.unwrap().unwrap();
    assert_eq!(metadata.created_at, "2020-01-01T00:00:00Z");

    // A later date is ignored.
    files::backdate_created_at(&pool, id, 1_609_459_200)
        .await
        .unwrap();
    let metadata = files::find_metadata(&pool, id).await.unwrap().unwrap();
    assert_eq!(metadata.created_at, "2020-01-01T00:00:00Z");
}

#[sqlx::test(migrations = "./migrations")]
async fn counts_image_and_thumbnail_views(pool: PgPool) {
    let uploader = common::test_user(&pool, "viewer").await.0.id;
    common::seed_files(&pool, uploader, "seen", 1, 1).await;
    common::seed_files(&pool, uploader, "unseen", 1, 1).await;
    let id = "seen000000";

    let views = ViewCounter::default();
    for (referrer, thumbnail) in [
        ("a.com", false),
        ("a.com", false),
        ("", true),
        ("b.com", false),
    ] {
        views.count(id, referrer, thumbnail);
    }
    views.count("missing0", "a.com", false);
    views.flush(&pool).await;
    // Flushed counts are cleared, so a second flush adds nothing.
    views.flush(&pool).await;

    let metadata = files::find_metadata(&pool, id).await.unwrap().unwrap();
    assert_eq!((metadata.views, metadata.thumbnail_views), (3, 1));
    // A second batch adds onto the existing referrer rows.
    views.count(id, "b.com", false);
    views.flush(&pool).await;
    let referrers: Vec<_> = files::find_referrer_views(&pool, id)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.referrer, row.views, row.thumbnail_views))
        .collect();
    assert_eq!(
        referrers,
        [
            ("a.com".into(), 2, 0),
            ("b.com".into(), 2, 0),
            (String::new(), 0, 1)
        ]
    );

    for sort_by in [ImagesSortBy::Views, ImagesSortBy::ThumbnailViews] {
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
        "abc12345.t.webp",
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
