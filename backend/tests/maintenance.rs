//! Finding images no longer linked from an application's database.

mod common;

use std::collections::HashSet;
use std::num::NonZeroU32;

use actix_web::web::Data;
use sqlx::PgPool;

use elysia::error::Error;
use elysia::repository::files::{ImagesSortBy, SortOrder};
use elysia::services::maintenance::{
    ScanParams, UnreferencedProgress, extract_ids, list_unreferenced, progress, start_scan,
    start_trash,
};

#[test]
fn extracts_image_thumbnail_and_viewer_links() {
    let mut ids = HashSet::new();
    extract_ids(
        "<img src=\"https://elysia.test/i/abc123.png\"> [url]http://elysia.test/t/def456.webp[/url] \
         elysia.test/i/ghi789 elysia.test/other/xyz elysia.test/i/ other.test/i/nope0000 \
         HTTPS://Elysia.Test/i/CaSe01 {\"url\":\"https:\\/\\/elysia.test\\/i\\/json01\"} \
         ?u=https%3A%2F%2Felysia.test%2Ft%2Fencoded1 \
         {\"{\\\"u\\\":\\\"https:\\\\/\\\\/elysia.test\\\\/i\\\\/array01\\\"}\"} \
         https://cdn.test/i/other01",
        &["elysia.test".into(), "cdn.test".into()],
        &mut ids,
    );
    assert_eq!(
        ids,
        HashSet::from(
            [
                "abc123", "def456", "ghi789", "CaSe01", "json01", "encoded1", "array01", "other01"
            ]
            .map(String::from)
        )
    );
}

/// Wait for the background scan or trash run to finish.
async fn finished() -> UnreferencedProgress {
    loop {
        let current = progress();
        if !current.running {
            return current;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

async fn unreferenced_ids(pool: &PgPool) -> Vec<String> {
    let page = NonZeroU32::new(1).unwrap();
    let (rows, _, _) = list_unreferenced(pool, page, 50, ImagesSortBy::CreatedAt, SortOrder::Desc)
        .await
        .unwrap();
    rows.into_iter().map(|row| row.id).collect()
}

/// Links in text, array and json columns count, `exclude` skips columns, and
/// trashing empties the list. The test database doubles as the scanned one.
#[sqlx::test(migrations = "./migrations", fixtures("maintenance_links"))]
async fn scans_and_trashes_unreferenced_images(pool: PgPool) {
    let (storage, _dir) = common::temp_storage();
    let uploader = common::test_user(&pool, "uploader").await.0.id;
    common::seed_files(&pool, uploader, "link", 5, 10).await;
    let mut database_url = url::Url::parse(&std::env::var("DATABASE_URL").unwrap()).unwrap();
    database_url.set_path(pool.connect_options().get_database().unwrap());
    let scan = |exclude: &[&str]| {
        start_scan(
            pool.clone(),
            ScanParams {
                database_url: database_url.to_string(),
                hosts: vec!["http://Elysia.test/".into()],
                exclude: exclude.iter().map(|entry| entry.to_string()).collect(),
            },
        )
    };

    scan(&[]).unwrap();
    let done = finished().await;
    assert_eq!(done.error, None);
    assert_eq!(done.referenced_images, 3);
    assert_eq!(unreferenced_ids(&pool).await, ["link000004", "link000003"]);

    scan(&["posts.body", "users"]).unwrap();
    assert_eq!(finished().await.referenced_images, 2);
    assert_eq!(unreferenced_ids(&pool).await.len(), 3);

    scan(&["posts.nope"]).unwrap();
    let failed = finished().await;
    assert!(failed.error.unwrap().contains("posts.nope"));
    assert!(matches!(
        start_scan(
            pool.clone(),
            ScanParams {
                database_url: "sqlite://elysia.db".into(),
                hosts: vec!["elysia.test".into()],
                exclude: Vec::new(),
            }
        ),
        Err(Error::BadRequest(_))
    ));
    assert!(matches!(
        start_scan(
            pool.clone(),
            ScanParams {
                database_url: database_url.to_string(),
                hosts: vec!["https://".into()],
                exclude: Vec::new(),
            }
        ),
        Err(Error::BadRequest(_))
    ));

    start_trash(pool.clone(), Data::new(storage), uploader)
        .await
        .unwrap();
    assert_eq!(finished().await.done, 3);
    assert!(unreferenced_ids(&pool).await.is_empty());
}
