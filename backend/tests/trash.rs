//! Trashing images: permissions, the moved files, the wipe and the stats.

mod common;

use std::sync::RwLock;

use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::{App, web::Data};
use sqlx::PgPool;
use uuid::Uuid;

use elysia::error::Error;
use elysia::repository::files::{self, ImagesQuery, ImagesStatus, SortOrder};
use elysia::repository::stats::{self, StatsInterval, StatsPeriod};
use elysia::routes;
use elysia::services::files::{
    TRASH_RETENTION_DAYS, delete_expired, get_metadata, list_images, restore, trash, wipe_trash,
};
use elysia::services::views::ViewCounter;

#[sqlx::test(migrations = "./migrations")]
async fn trashes_own_files(pool: PgPool) {
    let (storage, dir) = common::temp_storage();
    let owner = common::test_user(&pool, "owner").await.0.id;
    let other = common::test_user(&pool, "other").await.0.id;
    common::seed_files(&pool, owner, "mine", 2, 10).await;
    let (id, kept) = ("mine000000".to_string(), "mine000001");
    for file_id in [id.as_str(), kept] {
        storage.prepare_shard(file_id).await.unwrap();
        std::fs::write(storage.path_for(file_id, "png"), b"png").unwrap();
    }
    std::fs::write(storage.thumbnail_path_for(&id), b"webp").unwrap();
    let ids = std::slice::from_ref(&id);

    // Someone else's file is refused, and nothing moves.
    assert!(matches!(
        trash(&pool, &storage, other, false, ids, false).await,
        Err(Error::Forbidden)
    ));
    assert!(storage.path_for(&id, "png").exists());

    trash(&pool, &storage, owner, false, ids, false)
        .await
        .unwrap();
    assert!(files::find_available(&pool, ids).await.unwrap().is_empty());
    assert!(!storage.path_for(&id, "png").exists());
    assert!(!storage.thumbnail_path_for(&id).exists());
    assert!(dir.path().join("trash/mine000000.png").exists());
    assert!(dir.path().join("trash/mine000000.t.webp").exists());
    assert!(storage.path_for(kept, "png").exists());
    // Trashed files leave the browse page, the viewer page and the stored totals,
    // but still count as uploads.
    let (rows, total) = list_images(&pool, &common::page(1), None).await.unwrap();
    assert_eq!((rows.len(), total), (1, 1));
    // Only admins still find the viewer page.
    assert!(matches!(
        get_metadata(&pool, &id, false).await,
        Err(Error::NotFound)
    ));
    let metadata = get_metadata(&pool, &id, true).await.unwrap();
    assert_eq!(metadata.trashed_by_username.as_deref(), Some("owner"));
    assert_eq!(metadata.days_until_deletion, Some(TRASH_RETENTION_DAYS));
    let all_time = stats::period_totals(&pool, StatsPeriod::AllTime, StatsInterval::Day)
        .await
        .unwrap();
    let today = all_time.last().unwrap();
    assert_eq!(
        (
            today.files,
            today.bytes,
            today.uploads,
            today.uploaded_bytes,
            today.deletions,
            today.deleted_bytes
        ),
        (1, 10, 2, 20, 1, 10)
    );
    assert_eq!(stats::format_totals(&pool).await.unwrap()[0].files, 1);
    // The same bytes may be uploaded again: dedup only matches available files.
    assert!(
        files::find_record_by_hash(&pool, &id)
            .await
            .unwrap()
            .is_none()
    );

    // Trashing again is a no-op, even for an admin who does not own it.
    trash(&pool, &storage, other, true, ids, false)
        .await
        .unwrap();

    // Past the first hour, only an admin may trash it.
    files::backdate_created_at(&pool, kept, 1_577_836_800)
        .await
        .unwrap();
    let kept_ids = [kept.to_string()];
    assert!(matches!(
        trash(&pool, &storage, owner, false, &kept_ids, false).await,
        Err(Error::Forbidden)
    ));
    // Skipping the trash wipes the file at once.
    trash(&pool, &storage, other, true, &kept_ids, true)
        .await
        .unwrap();
    assert!(!dir.path().join("trash/mine000001.png").exists());
    assert!(matches!(
        get_metadata(&pool, kept, true).await,
        Err(Error::NotFound)
    ));
}

#[sqlx::test(migrations = "./migrations", fixtures("trash"))]
async fn wipes_trash(pool: PgPool) {
    let (storage, dir) = common::temp_storage();
    for file in ["expired0.png", "expired0.t.webp", "recent00.png"] {
        std::fs::write(dir.path().join("trash").join(file), b"bytes").unwrap();
    }
    let trashed_ids = || async {
        list_images(
            &pool,
            &ImagesQuery {
                order: SortOrder::Asc,
                status: ImagesStatus::Trashed,
                ..common::page(1)
            },
            None,
        )
        .await
        .unwrap()
        .0
        .into_iter()
        .map(|row| row.id)
        .collect::<Vec<_>>()
    };
    // Same upload date, so the id breaks the tie, descending.
    assert_eq!(trashed_ids().await, ["recent00", "expired0"]);
    // The trash view serves thumbnails from the trash, falling back to the file.
    assert!(
        storage
            .open_trashed_thumbnail("recent00", "png")
            .await
            .is_ok()
    );

    // The scheduled wipe only takes files past their 30 days.
    wipe_trash(&pool, &storage, TRASH_RETENTION_DAYS, None)
        .await
        .unwrap();
    assert_eq!(trashed_ids().await, ["recent00"]);
    assert!(!dir.path().join("trash/expired0.png").exists());
    assert!(!dir.path().join("trash/expired0.t.webp").exists());
    assert!(dir.path().join("trash/recent00.png").exists());

    // Emptying the trash takes the rest.
    let admin = common::test_user(&pool, "emptier").await.0.id;
    wipe_trash(&pool, &storage, 0, Some(admin)).await.unwrap();
    assert!(trashed_ids().await.is_empty());
    assert!(!dir.path().join("trash/recent00.png").exists());
}

/// Insert a 1x1 available png with the given hash.
async fn insert_png(pool: &PgPool, id: &str, hash: &str, uploader: Uuid) {
    insert_expiring_png(pool, id, hash, uploader, None).await;
}

/// Insert a 1x1 available png expiring after `expiry_days`.
async fn insert_expiring_png(
    pool: &PgPool,
    id: &str,
    hash: &str,
    uploader: Uuid,
    expiry_days: Option<i32>,
) {
    files::insert(
        pool,
        &files::NewFile {
            id,
            ext: "png",
            mime: "image/png",
            original_name: None,
            size: 1,
            width: 1,
            height: 1,
            hash,
            original_hash: hash,
            original_ext: "png",
            original_width: 1,
            original_height: 1,
            requested_quality: None,
            applied_quality: None,
            has_thumbnail: false,
            phash: 0,
            uploaded_by_user_id: uploader,
            api_key_id: None,
            expiry_days,
        },
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn deletes_expired_files(pool: PgPool) {
    let (storage, _dir) = common::temp_storage();
    let uploader = common::test_user(&pool, "expirer").await.0.id;
    insert_expiring_png(&pool, "expired0", "expired0", uploader, Some(0)).await;
    insert_expiring_png(&pool, "later000", "later000", uploader, Some(1)).await;
    insert_expiring_png(&pool, "shared00", "shared00", uploader, Some(0)).await;
    // Uploading shared00 again without expiry makes it never expire.
    files::extend_expiry(&pool, "shared00", None).await.unwrap();
    for id in ["expired0", "later000", "shared00"] {
        storage.prepare_shard(id).await.unwrap();
        std::fs::write(storage.path_for(id, "png"), b"png").unwrap();
    }

    delete_expired(&pool, &storage).await.unwrap();
    let ids = ["expired0", "later000", "shared00"].map(String::from);
    let available: Vec<String> = files::find_available(&pool, &ids)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(available, ["later000", "shared00"]);
    assert!(!storage.path_for("expired0", "png").exists());
    assert!(
        files::find_metadata(&pool, "later000")
            .await
            .unwrap()
            .unwrap()
            .expires_at
            .is_some()
    );
}

#[sqlx::test(migrations = "./migrations", fixtures("trash"))]
async fn restores_trashed_files(pool: PgPool) {
    let (storage, dir) = common::temp_storage();
    for file in ["expired0.png", "recent00.png", "recent00.t.webp"] {
        std::fs::write(dir.path().join("trash").join(file), b"bytes").unwrap();
    }
    let admin = common::test_user(&pool, "restorer").await.0.id;
    let available_ids = |ids: &'static [&'static str]| {
        let pool = &pool;
        async move {
            let ids: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
            files::find_available(pool, &ids)
                .await
                .unwrap()
                .into_iter()
                .map(|row| row.id)
                .collect::<Vec<_>>()
        }
    };

    // A trashed file missing from disk fails the whole restore, and the files
    // already moved go back to the trash.
    insert_png(&pool, "missing0", "zzz", admin).await;
    trash(
        &pool,
        &storage,
        admin,
        true,
        &["missing0".to_string()],
        false,
    )
    .await
    .unwrap();
    let ids = ["recent00".to_string(), "missing0".to_string()];
    assert!(restore(&pool, &storage, &ids).await.is_err());
    assert!(available_ids(&["recent00", "missing0"]).await.is_empty());
    assert!(dir.path().join("trash/recent00.png").exists());
    assert!(dir.path().join("trash/recent00.t.webp").exists());

    // Files past their retention stay trashed: the wipe is due.
    let ids = ["expired0".to_string(), "recent00".to_string()];
    restore(&pool, &storage, &ids).await.unwrap();
    assert_eq!(available_ids(&["expired0", "recent00"]).await, ["recent00"]);
    assert!(storage.path_for("recent00", "png").exists());
    assert!(storage.thumbnail_path_for("recent00").exists());
    assert!(dir.path().join("trash/expired0.png").exists());

    // Files whose bytes were uploaded again since stay trashed.
    trash(
        &pool,
        &storage,
        admin,
        true,
        &["recent00".to_string()],
        false,
    )
    .await
    .unwrap();
    insert_png(&pool, "reupload", "recent00", admin).await;
    restore(&pool, &storage, &["recent00".to_string()])
        .await
        .unwrap();
    assert!(available_ids(&["recent00"]).await.is_empty());
}

#[sqlx::test(migrations = "./migrations", fixtures("trash"))]
async fn serves_trashed_files_to_own_pages_only(pool: PgPool) {
    let (storage, dir) = common::temp_storage();
    std::fs::write(dir.path().join("trash/recent00.png"), b"bytes").unwrap();
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(common::test_config()))
            .app_data(Data::new(pool))
            .app_data(Data::new(storage))
            .app_data(Data::new(RwLock::new(common::test_settings())))
            .app_data(Data::new(ViewCounter::default()))
            .configure(routes::init),
    )
    .await;
    let status = |uri: &'static str, referer: &'static str| {
        let request = actix_test::TestRequest::get()
            .uri(uri)
            .insert_header(("Host", "elysia.test"))
            .insert_header(("Referer", referer))
            .to_request();
        let app = &app;
        async move { actix_test::call_service(app, request).await.status() }
    };

    for uri in ["/i/recent00.png", "/t/recent00.png"] {
        assert_eq!(
            status(uri, "https://elysia.test/browse").await,
            StatusCode::OK
        );
        assert_eq!(
            status(uri, "https://other.test/").await,
            StatusCode::NOT_FOUND
        );
    }
}
