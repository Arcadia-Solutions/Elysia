//! Fixtures generation: images land through the upload pipeline, with progress.
//!
//! Keep a single test here: runs share one process-wide progress state.

mod common;

use actix_web::web::Data;
use sqlx::PgPool;

use elysia::error::Error;
use elysia::services::files::list_images;
use elysia::services::fixtures::{self, FixturesParams};

fn params(image_count: u32) -> FixturesParams {
    FixturesParams {
        image_count,
        user_count: 2,
        history_days: 30,
        password: "password123".into(),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn generates_images_and_reports_progress(pool: PgPool) {
    let storage = Data::new(common::temp_storage().0);
    let settings = common::test_settings();
    assert!(matches!(
        fixtures::start(pool.clone(), storage.clone(), settings.clone(), params(0)),
        Err(Error::BadRequest(_))
    ));

    fixtures::start(pool.clone(), storage.clone(), settings.clone(), params(3)).unwrap();
    // A second run is refused while the first one goes.
    assert!(matches!(
        fixtures::start(pool.clone(), storage, settings, params(1)),
        Err(Error::Conflict)
    ));
    while fixtures::progress().running {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let progress = fixtures::progress();
    assert_eq!(
        (progress.done, progress.total, progress.error),
        (3, 3, None)
    );
    let (_, total) = list_images(&pool, &common::page(1), None).await.unwrap();
    assert_eq!(total, 3);
}
