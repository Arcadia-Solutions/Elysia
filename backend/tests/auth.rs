//! Local account auth: register, login, session checks, bans, ownership and
//! admin-only settings access.

mod common;

use std::sync::RwLock;

use actix_multipart::form::{MultipartFormConfig, tempfile::TempFileConfig};
use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::{self, Data};
use sqlx::PgPool;

use elysia::handlers::change_password;
use elysia::handlers::get_elysia_settings::get_elysia_settings;
use elysia::handlers::login::{self, LoginResponse};
use elysia::handlers::me::{self, ProfileResponse};
use elysia::handlers::register::{self, RegisterResponse};
use elysia::repository::{files, users};
use elysia::storage::Storage;

use common::*;

/// Auth routes plus the settings and upload routes that enforce auth.
macro_rules! auth_app {
    ($pool:expr) => {{ auth_app_with_settings!($pool, test_settings()) }};
}

/// Like `auth_app!` but with explicit settings (for disabled registration).
macro_rules! auth_app_with_settings {
    ($pool:expr, $settings:expr) => {{
        let upload_dir = std::env::temp_dir().join(format!(
            "elysia-auth-{}",
            elysia::services::files::generate_id()
        ));
        std::fs::create_dir_all(&upload_dir).unwrap();
        let storage = Storage::new(upload_dir.to_str().unwrap());
        actix_test::init_service(
            App::new()
                .app_data(Data::new(test_config()))
                .app_data(Data::new($pool))
                .app_data(Data::new(storage))
                .app_data(Data::new(RwLock::new($settings)))
                .app_data(MultipartFormConfig::default().total_limit(10 * 1024 * 1024))
                .app_data(TempFileConfig::default().directory(&upload_dir))
                .route("/api/auth/register", web::post().to(register::register))
                .route("/api/auth/login", web::post().to(login::login))
                .route("/api/auth/me", web::get().to(me::me))
                .route(
                    "/api/auth/password",
                    web::put().to(change_password::change_password),
                )
                .route("/api/elysia-settings", web::get().to(get_elysia_settings))
                .route("/api/upload", web::post().to(elysia::handlers::upload)),
        )
        .await
    }};
}

/// POST a registration, returning the status.
macro_rules! register_status {
    ($app:expr, $username:expr, $email:expr, $password:expr) => {{
        let request = actix_test::TestRequest::post()
            .uri("/api/auth/register")
            .set_json(serde_json::json!({
                "username": $username,
                "email": $email,
                "password": $password,
            }))
            .to_request();
        actix_test::call_service($app, request).await.status()
    }};
}

/// POST a login, returning the status.
macro_rules! login_status {
    ($app:expr, $login:expr, $password:expr) => {{
        let request = actix_test::TestRequest::post()
            .uri("/api/auth/login")
            .set_json(serde_json::json!({"login": $login, "password": $password}))
            .to_request();
        actix_test::call_service($app, request).await.status()
    }};
}

/// Successful login, returning the session token.
macro_rules! login_token {
    ($app:expr, $login:expr, $password:expr) => {{
        let request = actix_test::TestRequest::post()
            .uri("/api/auth/login")
            .set_json(serde_json::json!({"login": $login, "password": $password}))
            .to_request();
        let response: LoginResponse = actix_test::call_and_read_body_json($app, request).await;
        response.token
    }};
}

/// Successful login, returning the full response.
macro_rules! login_response {
    ($app:expr, $login:expr, $password:expr) => {{
        let request = actix_test::TestRequest::post()
            .uri("/api/auth/login")
            .set_json(serde_json::json!({"login": $login, "password": $password}))
            .to_request();
        let response: LoginResponse = actix_test::call_and_read_body_json($app, request).await;
        response
    }};
}

/// PUT a password change with a bearer token, returning the status.
macro_rules! change_password_status {
    ($app:expr, $token:expr, $current:expr, $new:expr) => {{
        let request = actix_test::TestRequest::put()
            .uri("/api/auth/password")
            .insert_header(("Authorization", format!("Bearer {}", $token)))
            .set_json(serde_json::json!({"current_password": $current, "new_password": $new}))
            .to_request();
        actix_test::call_service($app, request).await.status()
    }};
}

/// GET /api/auth/me with a bearer token, returning status + raw JSON body.
macro_rules! get_me {
    ($app:expr, $token:expr) => {{
        let request = actix_test::TestRequest::get()
            .uri("/api/auth/me")
            .insert_header(("Authorization", format!("Bearer {}", $token)))
            .to_request();
        let service_response = actix_test::call_service($app, request).await;
        let status = service_response.status();
        let profile: serde_json::Value = actix_test::read_body_json(service_response).await;
        (status, profile)
    }};
}

#[sqlx::test(migrations = "./migrations")]
async fn register_login_me_roundtrip(pool: PgPool) {
    let app = auth_app!(pool.clone());
    let request = actix_test::TestRequest::post()
        .uri("/api/auth/register")
        .set_json(serde_json::json!({
            "username": "alice",
            "email": "alice@example.com",
            "password": "supersecretpw",
        }))
        .to_request();
    let registered: RegisterResponse = actix_test::call_and_read_body_json(&app, request).await;
    assert_eq!(registered.username, "alice");
    assert_eq!(registered.email, "alice@example.com");
    // The migration seeds the admin; registrations never grant admin.
    assert!(!registered.is_admin);

    // Login works with the email as well as the username.
    let username_token = login_token!(&app, "alice", "supersecretpw");
    let email_token = login_token!(&app, "alice@example.com", "supersecretpw");
    assert!(!username_token.is_empty() && !email_token.is_empty());

    let (status, profile) = get_me!(&app, username_token);
    assert_eq!(status, StatusCode::OK);
    let profile: ProfileResponse = serde_json::from_value(profile).unwrap();
    assert_eq!(profile.id, registered.id);
    assert_eq!(profile.username, "alice");
    assert!(!profile.is_admin);
    assert_eq!(profile.provider, "local");
}

#[sqlx::test(migrations = "./migrations")]
async fn seeded_admin_can_login(pool: PgPool) {
    let app = auth_app!(pool);
    // Seeded by the migration: username `admin`, password `change-me-123`.
    let token = login_token!(&app, "admin", "change-me-123");
    let (status, profile) = get_me!(&app, token);
    assert_eq!(status, StatusCode::OK);
    let profile: ProfileResponse = serde_json::from_value(profile).unwrap();
    assert_eq!(profile.username, "admin");
    assert!(profile.is_admin);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_response_reports_caller_profile(pool: PgPool) {
    let app = auth_app!(pool.clone());
    assert_eq!(
        register_status!(&app, "alice", "alice@example.com", "supersecretpw12"),
        StatusCode::CREATED
    );
    let user = login_response!(&app, "alice", "supersecretpw12");
    assert!(!user.token.is_empty());
    assert_eq!(user.user.username, "alice");
    assert_eq!(user.user.email.as_deref(), Some("alice@example.com"));
    assert!(!user.user.is_admin);
    assert_eq!(user.user.provider, "local");
    assert!(!user.user.id.is_empty());
    let admin = login_response!(&app, "admin", "change-me-123");
    assert!(!admin.token.is_empty());
    assert_eq!(admin.user.username, "admin");
    assert!(admin.user.is_admin);
}

#[sqlx::test(migrations = "./migrations")]
async fn registration_can_be_disabled(pool: PgPool) {
    let settings = elysia::settings::ElysiaSettings {
        allow_registration: false,
        ..test_settings()
    };
    let app = auth_app_with_settings!(pool, settings);
    assert_eq!(
        register_status!(&app, "alice", "alice@example.com", "supersecretpw12"),
        StatusCode::FORBIDDEN
    );
    // The seeded admin still logs in while registration is closed.
    assert_eq!(
        login_status!(&app, "admin", "change-me-123"),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn second_user_is_not_admin(pool: PgPool) {
    let app = auth_app!(pool.clone());
    assert_eq!(
        register_status!(&app, "alice", "alice@example.com", "supersecretpw12"),
        StatusCode::CREATED
    );
    assert_eq!(
        register_status!(&app, "bob", "bob@example.com", "supersecretpw12"),
        StatusCode::CREATED
    );

    let token = login_token!(&app, "bob", "supersecretpw12");
    let (status, profile) = get_me!(&app, token);
    assert_eq!(status, StatusCode::OK);
    let profile: ProfileResponse = serde_json::from_value(profile).unwrap();
    assert!(!profile.is_admin);
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_register_conflicts(pool: PgPool) {
    let app = auth_app!(pool.clone());
    assert_eq!(
        register_status!(&app, "alice", "alice@example.com", "supersecretpw12"),
        StatusCode::CREATED
    );
    // Same username, fresh email.
    assert_eq!(
        register_status!(&app, "alice", "other@example.com", "supersecretpw12"),
        StatusCode::CONFLICT
    );
    // Same email, fresh username.
    assert_eq!(
        register_status!(&app, "alicia", "alice@example.com", "supersecretpw12"),
        StatusCode::CONFLICT
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn register_rejects_bad_input(pool: PgPool) {
    let app = auth_app!(pool.clone());
    // Too short, illegal characters, bad email, short password.
    for (username, email, password) in [
        ("ab", "ab@example.com", "supersecretpw12"),
        ("alice!", "alice@example.com", "supersecretpw12"),
        ("alice", "not-an-email", "supersecretpw12"),
        ("alice", "alice@example.com", "short"),
    ] {
        assert_eq!(
            register_status!(&app, username, email, password),
            StatusCode::BAD_REQUEST,
            "input should be rejected: {username} / {email}"
        );
    }
    // Nothing was stored beyond the seeded admin.
    assert_eq!(users::count_users(&pool).await.unwrap(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_rejects_unknown_user_and_wrong_password(pool: PgPool) {
    let app = auth_app!(pool.clone());
    assert_eq!(
        register_status!(&app, "alice", "alice@example.com", "supersecretpw12"),
        StatusCode::CREATED
    );
    assert_eq!(
        login_status!(&app, "nobody", "supersecretpw12"),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login_status!(&app, "alice", "wrongpassword!"),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn banned_user_is_blocked_until_unbanned(pool: PgPool) {
    let app = auth_app!(pool.clone());
    assert_eq!(
        register_status!(&app, "alice", "alice@example.com", "supersecretpw12"),
        StatusCode::CREATED
    );
    let token = login_token!(&app, "alice", "supersecretpw12");
    let row = users::find_by_username_or_email(&pool, "alice")
        .await
        .unwrap()
        .unwrap();

    users::set_banned(&pool, row.id, true).await.unwrap();
    assert_eq!(
        login_status!(&app, "alice", "supersecretpw12"),
        StatusCode::UNAUTHORIZED
    );
    let (status, _) = get_me!(&app, token);
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    users::set_banned(&pool, row.id, false).await.unwrap();
    assert_eq!(
        login_status!(&app, "alice", "supersecretpw12"),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn settings_require_an_admin_session(pool: PgPool) {
    let app = auth_app!(pool.clone());
    // The seeded admin reaches the admin-only settings endpoint.
    let admin_session = login_token!(&app, "admin", "change-me-123");
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-settings")
        .insert_header(("Authorization", format!("Bearer {admin_session}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK
    );

    // A non-admin JWT session is authenticated but forbidden there.
    let (_, user_token) = test_user(&pool, "mallory").await;
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-settings")
        .insert_header(("Authorization", format!("Bearer {user_token}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::FORBIDDEN
    );

    // No bearer at all is unauthorized.
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-settings")
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_records_the_uploading_user(pool: PgPool) {
    let app = auth_app!(pool.clone());
    assert_eq!(
        register_status!(&app, "alice", "alice@example.com", "supersecretpw12"),
        StatusCode::CREATED
    );
    let token = login_token!(&app, "alice", "supersecretpw12");
    let registered = users::find_by_username_or_email(&pool, "alice")
        .await
        .unwrap()
        .unwrap();

    let boundary = "authupload";
    let mut body = format!("--{boundary}\r\n").into_bytes();
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"x.png\"\r\n\
          Content-Type: image/png\r\n\r\n",
    );
    body.extend_from_slice(&tiny_png(2, 2));
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let request = actix_test::TestRequest::post()
        .uri("/api/upload")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(body)
        .to_request();
    let uploaded: serde_json::Value = actix_test::call_and_read_body_json(&app, request).await;
    let file_id = uploaded["id"].as_str().unwrap();

    assert_eq!(
        files::find(&pool, file_id)
            .await
            .unwrap()
            .unwrap()
            .uploaded_by_user_id,
        registered.id
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn registration_normalizes_case_and_whitespace(pool: PgPool) {
    let app = auth_app!(pool.clone());
    assert_eq!(
        register_status!(
            &app,
            "  Alice  ",
            "  Alice@Example.COM  ",
            "supersecretpw12"
        ),
        StatusCode::CREATED
    );
    let row = users::find_by_username_or_email(&pool, "Alice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.username, "Alice");
    assert_eq!(row.email.as_deref(), Some("alice@example.com"));

    // Login works regardless of email case or surrounding whitespace.
    assert_eq!(
        login_status!(&app, "ALICE@EXAMPLE.COM", "supersecretpw12"),
        StatusCode::OK
    );
    assert_eq!(
        login_status!(&app, "  alice@example.com  ", "supersecretpw12"),
        StatusCode::OK
    );

    // The normalized email is taken, in any case.
    assert_eq!(
        register_status!(&app, "alicia", "ALICE@example.com", "supersecretpw12"),
        StatusCode::CONFLICT
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn register_rejects_malformed_emails(pool: PgPool) {
    let app = auth_app!(pool.clone());
    for email in [
        "not-an-email",
        "a@b",
        "@example.com",
        "alice@",
        "alice@@example.com",
        "alice @example.com",
    ] {
        assert_eq!(
            register_status!(&app, "alice", email, "supersecretpw12"),
            StatusCode::BAD_REQUEST,
            "email should be rejected: {email}"
        );
    }
    assert_eq!(users::count_users(&pool).await.unwrap(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn password_change_roundtrip(pool: PgPool) {
    let app = auth_app!(pool.clone());
    assert_eq!(
        register_status!(&app, "alice", "alice@example.com", "supersecretpw12"),
        StatusCode::CREATED
    );
    let token = login_token!(&app, "alice", "supersecretpw12");

    // Wrong current password and weak replacements are rejected.
    assert_eq!(
        change_password_status!(&app, token, "wrongpassword!", "brandnewpassword12"),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        change_password_status!(&app, token, "supersecretpw12", "short"),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        login_status!(&app, "alice", "supersecretpw12"),
        StatusCode::OK
    );

    assert_eq!(
        change_password_status!(&app, token, "supersecretpw12", "brandnewpassword12"),
        StatusCode::OK
    );
    assert_eq!(
        login_status!(&app, "alice", "supersecretpw12"),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login_status!(&app, "alice", "brandnewpassword12"),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn password_change_requires_a_session(pool: PgPool) {
    let app = auth_app!(pool.clone());
    let request = actix_test::TestRequest::put()
        .uri("/api/auth/password")
        .set_json(
            serde_json::json!({"current_password": "x", "new_password": "brandnewpassword12"}),
        )
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );
}
