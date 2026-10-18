//! Browse pagination: page size, total count, the empty tail page and sorting.

mod common;

use sqlx::PgPool;

use elysia::handlers::parse_filename;
use elysia::repository::files::{self, ImagesSortBy, SortOrder};
use elysia::services::files::{IMAGES_PER_PAGE, list_images};
use elysia::services::views::ViewCounter;

#[sqlx::test(migrations = "./migrations")]
async fn paginates_with_total(pool: PgPool) {
    let per_page = IMAGES_PER_PAGE as usize;
    let uploader = common::test_user(&pool, "browser").await.0.id;
    common::seed_files(&pool, uploader, "id", per_page + 1, 1).await;

    let (first, total, first_page) =
        list_images(&pool, 1, ImagesSortBy::default(), SortOrder::default())
            .await
            .unwrap();
    assert_eq!(first.len(), per_page);
    assert_eq!(total, per_page as i64 + 1);
    assert_eq!(first_page, 1);
    assert!(first.iter().all(|row| row.total == total));

    let (second, _, _) = list_images(&pool, 2, ImagesSortBy::default(), SortOrder::default())
        .await
        .unwrap();
    assert_eq!(second.len(), 1);

    // Page below 1 clamps to the first page.
    let (clamped, _, clamped_page) =
        list_images(&pool, 0, ImagesSortBy::default(), SortOrder::default())
            .await
            .unwrap();
    assert_eq!(clamped.len(), per_page);
    assert_eq!(clamped_page, 1);

    // Past the last page: no rows, but the total still reflects every file.
    let (empty, empty_total, _) =
        list_images(&pool, 3, ImagesSortBy::default(), SortOrder::default())
            .await
            .unwrap();
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
            list_images(&pool, 1, sort_by, order).await.unwrap().0[0]
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
    for thumbnail in [false, false, true] {
        views.count(id, thumbnail);
    }
    views.count("missing0", false);
    views.flush(&pool).await;
    // Flushed counts are cleared, so a second flush adds nothing.
    views.flush(&pool).await;

    let metadata = files::find_metadata(&pool, id).await.unwrap().unwrap();
    assert_eq!((metadata.views, metadata.thumbnail_views), (2, 1));

    for sort_by in [ImagesSortBy::Views, ImagesSortBy::ThumbnailViews] {
        let rows = list_images(&pool, 1, sort_by, SortOrder::Desc)
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
