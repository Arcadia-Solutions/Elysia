//! Admin dashboard stats: running totals, format breakdown and uploader ranking.

mod common;

use sqlx::PgPool;

use elysia::repository::stats::{self, StatsInterval, StatsPeriod, period_totals};

#[sqlx::test(migrations = "./migrations")]
async fn aggregates_uploads(pool: PgPool) {
    let all_days = || period_totals(&pool, StatsPeriod::AllTime, StatsInterval::Day);
    let window = |period| stats::window_totals(&pool, period);
    // The seeded admin counts as a user even before any upload.
    assert!(all_days().await.unwrap().is_empty());
    let all_time = window(StatsPeriod::AllTime).await.unwrap();
    assert_eq!((all_time.new_users, all_time.active_uploaders), (1, 0));

    let small = common::test_user(&pool, "small").await.0.id;
    let big = common::test_user(&pool, "big").await.0.id;
    common::seed_files(&pool, small, "small", 3, 10).await;
    common::seed_files(&pool, big, "big", 1, 100).await;

    // Everything was uploaded today, so all time is a single day.
    let all_time = all_days().await.unwrap();
    assert_eq!(all_time.len(), 1);
    let today = &all_time[0];
    assert_eq!(
        (
            today.files,
            today.bytes,
            today.uploads,
            today.uploaded_bytes
        ),
        (4, 130, 4, 130)
    );
    let totals = window(StatsPeriod::AllTime).await.unwrap();
    assert_eq!(
        (
            totals.new_users,
            totals.active_uploaders,
            totals.uploads,
            totals.uploaded_bytes
        ),
        (3, 2, 4, 130)
    );

    // The last week has a step per day, empty ones included, ending on today.
    let last_week = period_totals(&pool, StatsPeriod::LastWeek, StatsInterval::Day)
        .await
        .unwrap();
    assert_eq!(last_week.len(), 8);
    assert_eq!((last_week[0].files, last_week[0].uploads), (0, 0));
    assert_eq!(last_week[7].bytes, 130);

    // An upload from before the last week still counts in the stored totals of
    // the series, but not in the window's totals.
    common::seed_files(&pool, big, "old", 1, 1000).await;
    sqlx::query("update files set created_at = now() - interval '8 days' where id like 'old%'")
        .execute(&pool)
        .await
        .unwrap();
    let by_year = period_totals(&pool, StatsPeriod::LastWeek, StatsInterval::Year)
        .await
        .unwrap();
    assert_eq!(by_year.last().unwrap().files, 5);
    let last_week = window(StatsPeriod::LastWeek).await.unwrap();
    assert_eq!((last_week.uploads, last_week.uploaded_bytes), (4, 130));

    let formats = stats::format_totals(&pool).await.unwrap();
    assert_eq!(formats.len(), 1);
    assert_eq!((formats[0].files, formats[0].bytes), (5, 1130));

    // Ranked by bytes, not file count.
    let all_time = stats::uploaders(&pool, StatsPeriod::AllTime).await.unwrap();
    assert_eq!(
        all_time
            .iter()
            .map(|row| (row.username.as_str(), row.files, row.bytes))
            .collect::<Vec<_>>(),
        [("big", 2, 1100), ("small", 3, 30)]
    );

    // Only uploads within the window count.
    let last_week = stats::uploaders(&pool, StatsPeriod::LastWeek)
        .await
        .unwrap();
    assert_eq!(
        last_week
            .iter()
            .map(|row| (row.username.as_str(), row.files, row.bytes))
            .collect::<Vec<_>>(),
        [("big", 1, 100), ("small", 3, 30)]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn groups_referrers_past_the_top_ones(pool: PgPool) {
    let uploader = common::test_user(&pool, "viewer").await.0.id;
    common::seed_files(&pool, uploader, "seen", 2, 1).await;
    common::seed_files(&pool, uploader, "old", 1, 1).await;
    sqlx::query("update files set created_at = now() - interval '30 days' where id like 'old%'")
        .execute(&pool)
        .await
        .unwrap();
    let views = elysia::services::views::ViewCounter::default();
    // Host n gets n + 1 views, split over both files and the thumbnail.
    for host in 0..12 {
        for view in 0..=host {
            let id = if view % 2 == 0 {
                "seen000000"
            } else {
                "seen000001"
            };
            views.count(id, &format!("h{host:02}.com"), view == 0);
        }
    }
    views.flush(&pool).await;
    // Views of a file uploaded a month ago: all time only.
    elysia::repository::files::add_views(
        &pool,
        &["old000000".into()],
        &["old.com".into()],
        &[100],
        &[0],
    )
    .await
    .unwrap();
    let top_referrer = |referrers: &[stats::ReferrerTotal]| referrers[0].referrer.clone();
    let all_time = stats::referrer_totals(&pool, StatsPeriod::AllTime)
        .await
        .unwrap();
    assert_eq!(top_referrer(&all_time).as_deref(), Some("old.com"));

    let referrers = stats::referrer_totals(&pool, StatsPeriod::LastWeek)
        .await
        .unwrap();
    assert_eq!(referrers.len(), 10);
    assert_eq!(
        (
            referrers[0].referrer.as_deref(),
            referrers[0].views,
            referrers[0].thumbnail_views
        ),
        (Some("h11.com"), 11, 1)
    );
    // h00 to h02 (1 + 2 + 3 views) are summed last, though their 6 views beat h03.
    let others = &referrers[9];
    assert_eq!(
        (
            others.referrer.as_deref(),
            others.views,
            others.thumbnail_views
        ),
        (None, 3, 3)
    );
}
