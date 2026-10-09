//! Browse pagination: page size, total count and the empty tail page.

mod common;

use sqlx::PgPool;

use elysia::repository::files::{self, NewFile};
use elysia::services::files::{IMAGES_PER_PAGE, list_images};

/// Insert `count` bare file rows with unique ids and hashes.
async fn seed(pool: &PgPool, count: usize) {
    for index in 0..count {
        let id = format!("id{index:06}");
        let hash = format!("hash{index:06}");
        files::insert(
            pool,
            &NewFile {
                id: &id,
                ext: "png",
                mime: "image/png",
                original_name: None,
                size: 1,
                width: 1,
                height: 1,
                hash: &hash,
                original_hash: &hash,
                original_ext: "png",
                original_width: 1,
                original_height: 1,
                requested_quality: None,
                applied_quality: None,
                has_thumbnail: false,
            },
        )
        .await
        .unwrap();
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn paginates_with_total(pool: PgPool) {
    let per_page = IMAGES_PER_PAGE as usize;
    seed(&pool, per_page + 1).await;

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
