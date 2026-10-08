//! Tests for the elysia-settings endpoints: an admin round-trip through
//! GET/PUT, persistence to the DB, and rejection of contradictory settings.

mod common;

use std::sync::RwLock;

use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::{self, Data};
use sqlx::PgPool;

use elysia::handlers::settings::{get_elysia_settings, put_elysia_settings};
use elysia::repository::settings as settings_repo;
use elysia::services::image::TargetFormat;
use elysia::settings::ElysiaSettings;

use common::test_config;

/// `(header-name, value)` carrying the test admin token.
fn auth_header() -> (&'static str, String) {
    (
        "Authorization",
        format!("Bearer {}", test_config().auth.admin_token),
    )
}

/// Build the two settings routes over `pool`, seeding shared state from the DB.
/// Inlined per test because actix's `init_service` return type is awkward to name.
macro_rules! settings_app {
    ($pool:expr) => {{
        let current = settings_repo::load(&$pool).await.unwrap();
        actix_test::init_service(
            App::new()
                .app_data(Data::new(test_config()))
                .app_data(Data::new($pool))
                .app_data(Data::new(RwLock::new(current)))
                .route("/api/elysia-settings", web::get().to(get_elysia_settings))
                .route("/api/elysia-settings", web::put().to(put_elysia_settings)),
        )
        .await
    }};
}

#[sqlx::test(migrations = "./migrations")]
async fn get_requires_admin_token(pool: PgPool) {
    let app = settings_app!(pool);
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-settings")
        .to_request();
    let status = actix_test::call_service(&app, request).await.status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_returns_the_seeded_defaults(pool: PgPool) {
    let app = settings_app!(pool);
    let (name, value) = auth_header();
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-settings")
        .insert_header((name, value))
        .to_request();
    let settings: ElysiaSettings = actix_test::call_and_read_body_json(&app, request).await;
    // The migration seeds a 10 MiB ceiling with processing off.
    assert_eq!(settings.max_file_size_bytes, 10 * 1024 * 1024);
    assert!(settings.target_file_format.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn put_persists_and_reflects_new_settings(pool: PgPool) {
    let app = settings_app!(pool.clone());
    let (name, value) = auth_header();
    let request = actix_test::TestRequest::put()
        .uri("/api/elysia-settings")
        .insert_header((name, value))
        .set_json(serde_json::json!({
            "max_file_size_bytes": 2048,
            "max_width_pixels": 4000,
            "max_height_pixels": 4000,
            "target_file_format": "webp",
        }))
        .to_request();
    let saved: ElysiaSettings = actix_test::call_and_read_body_json(&app, request).await;
    assert_eq!(saved.max_file_size_bytes, 2048);
    assert_eq!(saved.target_file_format, Some(TargetFormat::Webp));

    // The change is persisted, not just held in memory.
    let reloaded = settings_repo::load(&pool).await.unwrap();
    assert_eq!(reloaded.max_file_size_bytes, 2048);
    assert_eq!(reloaded.max_width_pixels, 4000);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_rejects_contradictory_settings(pool: PgPool) {
    let app = settings_app!(pool.clone());
    let (name, value) = auth_header();
    // A target format with no decode caps is a decompression-bomb risk.
    let request = actix_test::TestRequest::put()
        .uri("/api/elysia-settings")
        .insert_header((name, value))
        .set_json(serde_json::json!({
            "max_file_size_bytes": 2048,
            "target_file_format": "webp",
        }))
        .to_request();
    let status = actix_test::call_service(&app, request).await.status();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The rejected save left the stored row untouched.
    let reloaded = settings_repo::load(&pool).await.unwrap();
    assert!(reloaded.target_file_format.is_none());
}
