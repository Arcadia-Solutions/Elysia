//! Browse pagination: page size, total count and the empty tail page.

mod common;

use sqlx::PgPool;

use elysia::services::files::{IMAGES_PER_PAGE, list_images};

#[sqlx::test(migrations = "./migrations")]
async fn paginates_with_total(pool: PgPool) {
    let per_page = IMAGES_PER_PAGE as usize;
    let uploader = common::test_user(&pool, "browser").await.0.id;
    common::seed_files(&pool, uploader, "id", per_page + 1, 1).await;

    let (first, total, first_page) = list_images(&pool, 1).await.unwrap();
    assert_eq!(first.len(), per_page);
    assert_eq!(total, per_page as i64 + 1);
    assert_eq!(first_page, 1);
    assert!(first.iter().all(|row| row.total == total));

    let (second, _, _) = list_images(&pool, 2).await.unwrap();
    assert_eq!(second.len(), 1);

    // Page below 1 clamps to the first page.
    let (clamped, _, clamped_page) = list_images(&pool, 0).await.unwrap();
    assert_eq!(clamped.len(), per_page);
    assert_eq!(clamped_page, 1);

    // Past the last page: no rows, but the total still reflects every file.
    let (empty, empty_total, _) = list_images(&pool, 3).await.unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty_total, per_page as i64 + 1);
}
