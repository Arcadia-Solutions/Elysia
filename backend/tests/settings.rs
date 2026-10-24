//! Tests for the elysia-settings endpoints: an admin round-trip through
//! GET/PUT, persistence to the DB, and rejection of contradictory settings.

mod common;

use std::sync::RwLock;

use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use sqlx::PgPool;

use elysia::repository::{settings as settings_repo, users};
use elysia::routes;
use elysia::services::auth as auth_service;
use elysia::services::image::TargetFormat;
use elysia::settings::{ElysiaSettings, RefererHost, referer_host};

use common::test_config;

/// `(header-name, value)` carrying a session token for the seeded admin.
async fn auth_header(pool: &PgPool) -> (&'static str, String) {
    let admin = users::find_by_username_or_email(pool, "admin")
        .await
        .unwrap()
        .expect("migration seeds a default admin");
    assert!(admin.is_admin);
    let token = auth_service::mint_jwt(&admin.id, &test_config().auth.jwt_secret).unwrap();
    ("Authorization", format!("Bearer {token}"))
}

/// Build the app over `pool`, seeding shared settings state from the DB.
/// Inlined per test because actix's `init_service` return type is awkward to name.
macro_rules! settings_app {
    ($pool:expr) => {{
        let current = settings_repo::load(&$pool).await.unwrap();
        actix_test::init_service(
            App::new()
                .app_data(Data::new(test_config()))
                .app_data(Data::new($pool))
                .app_data(Data::new(RwLock::new(current)))
                .configure(routes::init),
        )
        .await
    }};
}

#[sqlx::test(migrations = "./migrations")]
async fn get_requires_an_admin_session(pool: PgPool) {
    let app = settings_app!(pool);
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-settings")
        .to_request();
    let status = actix_test::call_service(&app, request).await.status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn public_settings_require_a_session(pool: PgPool) {
    let app = settings_app!(pool.clone());
    let request = actix_test::TestRequest::get()
        .uri("/api/public-elysia-settings")
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let (name, value) = auth_header(&pool).await;
    let request = actix_test::TestRequest::get()
        .uri("/api/public-elysia-settings")
        .insert_header((name, value))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn unauth_settings_are_open_and_registration_only(pool: PgPool) {
    let app = settings_app!(pool);
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-unauth-settings")
        .to_request();
    let body: serde_json::Value = actix_test::call_and_read_body_json(&app, request).await;
    assert_eq!(
        body.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["allow_registration", "sso_provider_name"]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn get_returns_the_seeded_defaults(pool: PgPool) {
    let app = settings_app!(pool.clone());
    let (name, value) = auth_header(&pool).await;
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-settings")
        .insert_header((name, value))
        .to_request();
    let settings: ElysiaSettings = actix_test::call_and_read_body_json(&app, request).await;
    // The migration seeds a 25 MiB ceiling, WebP at quality 75, thumbnails on,
    // EXIF stripped and registration closed, and that row is valid.
    assert_eq!(settings.max_file_size_bytes, 25 * 1024 * 1024);
    assert_eq!(
        settings.default_target_file_format,
        Some(TargetFormat::Webp)
    );
    assert_eq!(settings.default_compression, 75);
    assert!(!settings.allow_overriding_compression);
    assert!(settings.thumbnails_enabled());
    assert!(settings.strip_exif_by_default);
    assert!(!settings.allow_registration);
    settings.validate().unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn put_persists_and_reflects_new_settings(pool: PgPool) {
    let app = settings_app!(pool.clone());
    let (name, value) = auth_header(&pool).await;
    let request = actix_test::TestRequest::put()
        .uri("/api/elysia-settings")
        .insert_header((name, value))
        .set_json(serde_json::json!({
            "max_file_size_bytes": 2048,
            "max_width_pixels": 4000,
            "max_height_pixels": 4000,
            "default_target_file_format": "webp",
        }))
        .to_request();
    let saved: ElysiaSettings = actix_test::call_and_read_body_json(&app, request).await;
    assert_eq!(saved.max_file_size_bytes, 2048);
    assert_eq!(saved.default_target_file_format, Some(TargetFormat::Webp));

    // The change is persisted, not just held in memory.
    let reloaded = settings_repo::load(&pool).await.unwrap();
    assert_eq!(reloaded.max_file_size_bytes, 2048);
    assert_eq!(reloaded.max_width_pixels, 4000);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_rejects_contradictory_settings(pool: PgPool) {
    let app = settings_app!(pool.clone());
    let (name, value) = auth_header(&pool).await;
    // A target format with no decode caps is a decompression-bomb risk.
    let request = actix_test::TestRequest::put()
        .uri("/api/elysia-settings")
        .insert_header((name, value))
        .set_json(serde_json::json!({
            "max_file_size_bytes": 2048,
            "default_target_file_format": "webp",
        }))
        .to_request();
    let status = actix_test::call_service(&app, request).await.status();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The rejected save left the stored row untouched.
    let reloaded = settings_repo::load(&pool).await.unwrap();
    assert_eq!(reloaded.max_file_size_bytes, 25 * 1024 * 1024);
}

#[test]
fn referrer_allowlist() {
    let mut settings = common::test_settings();
    // Empty list: anything goes, even no Referer.
    assert!(settings.referrer_allowed(None, "elysia.test"));

    settings.allowed_referrers = vec![" Forum.Example ".into(), "".into(), "forum.example".into()];
    settings.normalize();
    assert_eq!(settings.allowed_referrers, vec!["forum.example"]);

    let allowed = |referer| settings.referrer_allowed(referer, "elysia.test:8090");
    assert!(allowed(Some("https://forum.example/thread/1")));
    assert!(allowed(Some("http://FORUM.example:8080/")));
    assert!(
        allowed(Some("http://elysia.test:5173/browse")),
        "own host, any port"
    );
    assert!(!allowed(None));
    assert!(!allowed(Some("not a url")));
    assert!(!allowed(Some("https://evil.example/")));
    assert!(!allowed(Some("https://forum.example.evil.test/")));
}

#[test]
fn own_pages_are_told_apart_from_other_referrers() {
    let is_own = |referer| {
        matches!(
            referer_host(referer, "Elysia.test:8090"),
            Some(RefererHost::Own)
        )
    };
    assert!(
        is_own(Some("http://elysia.test:5174/browse")),
        "port ignored"
    );
    assert!(!is_own(Some("https://forum.example/thread/1")));
    assert!(!is_own(Some("https://elysia.test.evil.example/")));
    assert!(!is_own(None));
}

#[test]
fn referrer_entries_must_be_hostnames() {
    let mut settings = common::test_settings();
    settings.max_file_size_bytes = 1;
    settings.allowed_referrers = vec!["https://forum.example/".into()];
    assert!(
        settings
            .validate()
            .unwrap_err()
            .contains("allowed_referrers")
    );
    settings.allowed_referrers = vec!["forum.example".into()];
    assert!(settings.validate().is_ok());
}
