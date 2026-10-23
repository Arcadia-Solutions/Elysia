//! OpenID Connect client: discovery, authorize URL, callback provisioning,
//! conflict handling and live ban checks against a mock provider.

mod common;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::{self, Data, Form, Path};
use actix_web::{App, HttpRequest, HttpResponse};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use sqlx::PgPool;

use elysia::handlers::auth::login::LoginResponse;
use elysia::repository::users;
use elysia::routes;
use elysia::services::auth as auth_service;
use elysia::services::sso::SsoState;

use common::*;

const MOCK_CODE: &str = "mock-code";
const MOCK_ACCESS_TOKEN: &str = "mock-access-token";
const MOCK_SUB: &str = "42";

/// Minimal Arcadia stand-in: discovery, token, userinfo and user status
/// endpoints with knobs for what the provider answers.
struct MockIdp {
    issuer: String,
    active: AtomicBool,
    username: std::sync::Mutex<String>,
    email: std::sync::Mutex<Option<String>>,
}

/// Whether the request carries the test client's Basic credentials.
fn client_authenticated(request: &HttpRequest) -> bool {
    let expected = format!(
        "Basic {}",
        STANDARD.encode("elysia-test-client:elysia-test-secret")
    );
    request
        .headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        == Some(expected.as_str())
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: String,
    redirect_uri: String,
    code_verifier: String,
}

async fn mock_discovery(mock: Data<MockIdp>) -> HttpResponse {
    let issuer = &mock.issuer;
    HttpResponse::Ok().json(serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "userinfo_endpoint": format!("{issuer}/userinfo"),
    }))
}

async fn mock_token(request: HttpRequest, form: Form<TokenForm>) -> HttpResponse {
    let valid = client_authenticated(&request)
        && form.grant_type == "authorization_code"
        && form.code == MOCK_CODE
        && form.redirect_uri == "http://localhost:3000/auth/sso/callback"
        && !form.code_verifier.is_empty();
    if !valid {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid_grant"}));
    }
    HttpResponse::Ok().json(serde_json::json!({
        "access_token": MOCK_ACCESS_TOKEN,
        "token_type": "Bearer",
        "expires_in": 3600,
    }))
}

async fn mock_userinfo(mock: Data<MockIdp>, request: HttpRequest) -> HttpResponse {
    let authorized = request
        .headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        == Some(&format!("Bearer {MOCK_ACCESS_TOKEN}"));
    if !authorized {
        return HttpResponse::Unauthorized().finish();
    }
    HttpResponse::Ok().json(serde_json::json!({
        "sub": MOCK_SUB,
        "preferred_username": *mock.username.lock().unwrap(),
        "email": *mock.email.lock().unwrap(),
    }))
}

async fn mock_status(
    mock: Data<MockIdp>,
    request: HttpRequest,
    subject: Path<String>,
) -> HttpResponse {
    if !client_authenticated(&request) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "invalid_client"}));
    }
    let active = mock.active.load(Ordering::SeqCst) && *subject == MOCK_SUB;
    HttpResponse::Ok().json(serde_json::json!({ "active": active }))
}

/// Boot a mock Arcadia on a loopback port, returning its issuer URL (the
/// endpoints hang directly off it) and its knobs.
async fn spawn_mock() -> (String, Data<MockIdp>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let mock = Data::new(MockIdp {
        issuer: issuer.clone(),
        active: AtomicBool::new(true),
        username: std::sync::Mutex::new("arcadia-bob".into()),
        email: std::sync::Mutex::new(Some("bob@arcadia.example".into())),
    });
    let data = mock.clone();
    tokio::spawn(
        actix_web::HttpServer::new(move || {
            App::new()
                .app_data(data.clone())
                .route(
                    "/.well-known/openid-configuration",
                    web::get().to(mock_discovery),
                )
                .route("/token", web::post().to(mock_token))
                .route("/userinfo", web::get().to(mock_userinfo))
                .route("/users/{subject}/status", web::get().to(mock_status))
        })
        .listen(listener)
        .unwrap()
        .run(),
    );
    (issuer, mock)
}

/// The app wired with SSO state, so the SSO and session-guarded routes work.
macro_rules! sso_app {
    ($pool:expr, $config:expr) => {{
        actix_test::init_service(
            App::new()
                .app_data(Data::new($config))
                .app_data(Data::new($pool))
                .app_data(Data::new(SsoState::default()))
                .app_data(Data::new(std::sync::RwLock::new(common::test_settings())))
                .configure(routes::init),
        )
        .await
    }};
}

/// Start a login, returning the PKCE verifier the callback must send back
/// and the authorize URL.
macro_rules! start_login {
    ($app:expr) => {{
        let request = actix_test::TestRequest::get()
            .uri("/api/auth/sso/login")
            .to_request();
        let response = actix_test::call_service($app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = actix_test::read_body_json(response).await;
        (
            body["code_verifier"].as_str().unwrap().to_string(),
            body["authorization_url"].as_str().unwrap().to_string(),
        )
    }};
}

/// Finish a login through the callback, returning status + raw JSON body.
macro_rules! finish_login {
    ($app:expr, $code_verifier:expr) => {{
        let request = actix_test::TestRequest::post()
            .uri("/api/auth/sso/callback")
            .set_json(serde_json::json!({"code": MOCK_CODE, "code_verifier": $code_verifier}))
            .to_request();
        let response = actix_test::call_service($app, request).await;
        let status = response.status();
        let body: serde_json::Value = actix_test::read_body_json(response).await;
        (status, body)
    }};
}

/// GET /api/auth/me with a bearer token, returning the status.
macro_rules! me_status {
    ($app:expr, $token:expr) => {{
        let request = actix_test::TestRequest::get()
            .uri("/api/auth/me")
            .insert_header(("Authorization", format!("Bearer {}", $token)))
            .to_request();
        actix_test::call_service($app, request).await.status()
    }};
}

#[sqlx::test(migrations = "./migrations")]
async fn disabled_sso_rejects_login_and_callback(pool: PgPool) {
    let app = sso_app!(pool, test_config());
    let request = actix_test::TestRequest::get()
        .uri("/api/auth/sso/login")
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::BAD_REQUEST
    );
    let request = actix_test::TestRequest::post()
        .uri("/api/auth/sso/callback")
        .set_json(serde_json::json!({"code": "x", "code_verifier": "y"}))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn login_builds_authorize_url_with_pkce(pool: PgPool) {
    let (issuer, _mock) = spawn_mock().await;
    let app = sso_app!(pool, sso_config(&issuer));
    let (code_verifier, authorization_url) = start_login!(&app);

    let url = url::Url::parse(&authorization_url).unwrap();
    assert_eq!(url.path(), "/authorize");
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
    assert_eq!(
        query.get("client_id").map(String::as_str),
        Some("elysia-test-client")
    );
    assert_eq!(
        query.get("redirect_uri").map(String::as_str),
        Some("http://localhost:3000/auth/sso/callback")
    );
    assert_eq!(
        query.get("scope").map(String::as_str),
        Some("openid profile email")
    );
    assert!(query.get("state").is_some_and(|state| !state.is_empty()));
    assert_eq!(
        query.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    // The challenge is the S256 hash of the verifier, which itself stays out
    // of the URL; the host matches the configured issuer.
    assert_eq!(
        query.get("code_challenge"),
        Some(&elysia::services::sso::code_challenge(&code_verifier))
    );
    assert!(!authorization_url.contains(&code_verifier));
    assert_eq!(url.host_str(), url::Url::parse(&issuer).unwrap().host_str());
}

#[sqlx::test(migrations = "./migrations")]
async fn callback_provisions_user_and_relogin_reuses_row(pool: PgPool) {
    let (issuer, _mock) = spawn_mock().await;
    let app = sso_app!(pool.clone(), sso_config(&issuer));

    let (code_verifier, _) = start_login!(&app);
    let (status, body) = finish_login!(&app, code_verifier);
    assert_eq!(status, StatusCode::OK);
    let first: LoginResponse = serde_json::from_value(body).unwrap();
    assert!(!first.token.is_empty());
    assert_eq!(first.user.username, "arcadia-bob");
    assert_eq!(first.user.email.as_deref(), Some("bob@arcadia.example"));
    assert_eq!(first.user.provider, "sso");
    assert_eq!(users::count_users(&pool, None).await.unwrap(), 2);

    // A second login provisions nothing new and keeps the same row.
    let (code_verifier, _) = start_login!(&app);
    let (status, body) = finish_login!(&app, code_verifier);
    assert_eq!(status, StatusCode::OK);
    let second: LoginResponse = serde_json::from_value(body).unwrap();
    assert_eq!(second.user.id, first.user.id);
    assert_eq!(second.user.username, "arcadia-bob");
    assert_eq!(users::count_users(&pool, None).await.unwrap(), 2);

    // The minted session passes the live ban check.
    assert_eq!(me_status!(&app, first.token), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn callback_refuses_locally_banned_user(pool: PgPool) {
    let (issuer, _mock) = spawn_mock().await;
    let app = sso_app!(pool.clone(), sso_config(&issuer));
    let (code_verifier, _) = start_login!(&app);
    let (_, body) = finish_login!(&app, code_verifier);
    let login: LoginResponse = serde_json::from_value(body).unwrap();
    users::set_banned(&pool, login.user.id.parse().unwrap(), true)
        .await
        .unwrap();
    let (code_verifier, _) = start_login!(&app);
    assert_eq!(
        finish_login!(&app, code_verifier).0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn callback_refuses_provider_banned_user(pool: PgPool) {
    let (issuer, mock) = spawn_mock().await;
    mock.active.store(false, Ordering::SeqCst);
    let app = sso_app!(pool, sso_config(&issuer));
    let (code_verifier, _) = start_login!(&app);
    assert_eq!(
        finish_login!(&app, code_verifier).0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn conflicting_username_and_email_are_handled(pool: PgPool) {
    let (issuer, mock) = spawn_mock().await;
    // A local account already owns the provider's username and email.
    let password_hash = auth_service::hash_password("long-test-password").unwrap();
    users::create_local_user(
        &pool,
        "arcadia-bob",
        Some("bob@arcadia.example"),
        &password_hash,
        false,
    )
    .await
    .unwrap();
    *mock.username.lock().unwrap() = "arcadia-bob".into();
    *mock.email.lock().unwrap() = Some("bob@arcadia.example".into());

    let app = sso_app!(pool.clone(), sso_config(&issuer));
    let (code_verifier, _) = start_login!(&app);
    let (status, body) = finish_login!(&app, code_verifier);
    assert_eq!(status, StatusCode::OK);
    let login: LoginResponse = serde_json::from_value(body).unwrap();
    // The username is deconflicted, the clashing email is dropped to NULL.
    assert_ne!(login.user.username, "arcadia-bob");
    assert!(login.user.username.starts_with("arcadia-bob-"));
    assert_eq!(login.user.email, None);
    assert_eq!(login.user.provider, "sso");
}

#[sqlx::test(migrations = "./migrations")]
async fn unusable_provider_username_falls_back(pool: PgPool) {
    let (issuer, mock) = spawn_mock().await;
    *mock.username.lock().unwrap() = "  !!  ".into();
    *mock.email.lock().unwrap() = None;

    let app = sso_app!(pool, sso_config(&issuer));
    let (code_verifier, _) = start_login!(&app);
    let (status, body) = finish_login!(&app, code_verifier);
    assert_eq!(status, StatusCode::OK);
    let login: LoginResponse = serde_json::from_value(body).unwrap();
    assert!(auth_service::valid_username(&login.user.username));
    assert_eq!(login.user.email, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn arcadia_ban_blocks_session_but_not_local_users(pool: PgPool) {
    let (issuer, mock) = spawn_mock().await;
    let app = sso_app!(pool.clone(), sso_config(&issuer));

    let (code_verifier, _) = start_login!(&app);
    let (status, body) = finish_login!(&app, code_verifier);
    assert_eq!(status, StatusCode::OK);
    let login: LoginResponse = serde_json::from_value(body).unwrap();
    assert_eq!(me_status!(&app, login.token), StatusCode::OK);

    // Arcadia bans the user: a fresh app (empty status cache) rejects them.
    mock.active.store(false, Ordering::SeqCst);
    let blocked = sso_app!(pool.clone(), sso_config(&issuer));
    assert_eq!(me_status!(&blocked, login.token), StatusCode::UNAUTHORIZED);

    // Local users never touch the provider: no network, no ban check.
    let (_, local_token) = test_user(&pool, "local-larry").await;
    assert_eq!(me_status!(&blocked, local_token), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn unreachable_arcadia_fails_closed(pool: PgPool) {
    let (issuer, _mock) = spawn_mock().await;
    let app = sso_app!(pool.clone(), sso_config(&issuer));
    let (code_verifier, _) = start_login!(&app);
    let (status, body) = finish_login!(&app, code_verifier);
    assert_eq!(status, StatusCode::OK);
    let login: LoginResponse = serde_json::from_value(body).unwrap();

    // Nothing listens on port 1: the status check cannot answer.
    let unreachable = sso_app!(pool, sso_config("http://127.0.0.1:1"));
    assert_eq!(
        me_status!(&unreachable, login.token),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn discovery_with_another_issuer_is_refused(pool: PgPool) {
    let (issuer, _mock) = spawn_mock().await;
    // Same server, but the configured issuer differs from the one the
    // discovery document names.
    let app = sso_app!(pool, sso_config(&issuer.replace("127.0.0.1", "localhost")));
    let request = actix_test::TestRequest::get()
        .uri("/api/auth/sso/login")
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn no_status_endpoint_skips_the_ban_check(pool: PgPool) {
    let (issuer, mock) = spawn_mock().await;
    let mut config = sso_config(&issuer);
    config.auth.sso.as_mut().unwrap().status_endpoint = None;
    let app = sso_app!(pool, config);
    let (code_verifier, _) = start_login!(&app);
    let (status, body) = finish_login!(&app, code_verifier);
    assert_eq!(status, StatusCode::OK);
    let login: LoginResponse = serde_json::from_value(body).unwrap();

    // A provider without a status endpoint cannot ban: the session holds.
    mock.active.store(false, Ordering::SeqCst);
    assert_eq!(me_status!(&app, login.token), StatusCode::OK);
}
