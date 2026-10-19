//! Trashing images: permissions, the moved files, the wipe and the stats.

mod common;

use sqlx::PgPool;

use elysia::error::Error;
use elysia::repository::files::{self, ImagesQuery, ImagesStatus, SortOrder};
use elysia::repository::stats::{self, StatsInterval, StatsPeriod};
use elysia::services::files::{TRASH_RETENTION_DAYS, list_images, trash, wipe_trash};

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
        trash(&pool, &storage, other, false, ids).await,
        Err(Error::Forbidden)
    ));
    assert!(storage.path_for(&id, "png").exists());

    trash(&pool, &storage, owner, false, ids).await.unwrap();
    assert!(files::find_available(&pool, ids).await.unwrap().is_empty());
    assert!(!storage.path_for(&id, "png").exists());
    assert!(!storage.thumbnail_path_for(&id).exists());
    assert!(dir.join("trash/mine000000.png").exists());
    assert!(dir.join("trash/mine000000.t.webp").exists());
    assert!(storage.path_for(kept, "png").exists());
    // Trashed files leave the browse page, the viewer page and the stored totals,
    // but still count as uploads.
    let (rows, total) = list_images(&pool, &common::page(1)).await.unwrap();
    assert_eq!((rows.len(), total), (1, 1));
    assert!(files::find_metadata(&pool, &id).await.unwrap().is_none());
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
    trash(&pool, &storage, other, true, ids).await.unwrap();

    // Past the first hour, only an admin may trash it.
    files::backdate_created_at(&pool, kept, 1_577_836_800)
        .await
        .unwrap();
    let kept_ids = [kept.to_string()];
    assert!(matches!(
        trash(&pool, &storage, owner, false, &kept_ids).await,
        Err(Error::Forbidden)
    ));
    trash(&pool, &storage, other, true, &kept_ids)
        .await
        .unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}

#[sqlx::test(migrations = "./migrations", fixtures("trash"))]
async fn wipes_trash(pool: PgPool) {
    let (storage, dir) = common::temp_storage();
    for file in ["expired0.png", "expired0.t.webp", "recent00.png"] {
        std::fs::write(dir.join("trash").join(file), b"bytes").unwrap();
    }
    let trashed_ids = || async {
        list_images(
            &pool,
            &ImagesQuery {
                order: SortOrder::Asc,
                status: ImagesStatus::Trashed,
                ..common::page(1)
            },
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
    assert!(!dir.join("trash/expired0.png").exists());
    assert!(!dir.join("trash/expired0.t.webp").exists());
    assert!(dir.join("trash/recent00.png").exists());

    // Emptying the trash takes the rest.
    let admin = common::test_user(&pool, "emptier").await.0.id;
    wipe_trash(&pool, &storage, 0, Some(admin)).await.unwrap();
    assert!(trashed_ids().await.is_empty());
    assert!(!dir.join("trash/recent00.png").exists());

    let _ = std::fs::remove_dir_all(&dir);
}
