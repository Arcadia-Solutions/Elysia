//! Rate limiting of unauthenticated routes per client IP (behind trusted
//! proxies too), of login per IP, and of authenticated routes per user.

mod common;

use std::net::SocketAddr;
use std::sync::RwLock;

use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use sqlx::PgPool;

use elysia::config::RateLimitConfig;
use elysia::middlewares::RateLimits;
use elysia::repository::api_keys::{self, ApiKeyScope};
use elysia::routes;
use elysia::services::auth as auth_service;
use elysia::services::views::ViewCounter;
use elysia::storage::Storage;

use common::*;

/// Limits with every budget off except the given ones.
fn limits(
    unauthenticated_per_ip: u32,
    login_per_ip: u32,
    authenticated_per_user: u32,
) -> RateLimits {
    RateLimits::new(Some(&RateLimitConfig {
        unauthenticated_requests_per_minute_per_ip: unauthenticated_per_ip,
        login_attempts_per_minute_per_ip: login_per_ip,
        authenticated_requests_per_minute_per_user: authenticated_per_user,
        trusted_proxies: vec![
            "10.0.0.0/8".parse().unwrap(),
            "192.168.0.0/16".parse().unwrap(),
        ],
    }))
}

/// The full app with the given limits, as `main` builds it.
macro_rules! limited_app {
    ($pool:expr, $limits:expr) => {{
        let upload_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(upload_dir.path().to_str().unwrap());
        actix_test::init_service(
            App::new()
                .app_data(Data::new(test_config()))
                .app_data(Data::new($pool))
                .app_data(upload_dir)
                .app_data(Data::new(storage))
                .app_data(Data::new(RwLock::new(test_settings())))
                .app_data(Data::new(ViewCounter::default()))
                .app_data(Data::new($limits))
                .configure(routes::init),
        )
        .await
    }};
}

/// GET the unauthenticated settings from `peer` with an optional `X-Forwarded-For`, returning the status.
macro_rules! status_from {
    ($app:expr, $peer:expr, $forwarded_for:expr) => {{
        let mut request = actix_test::TestRequest::get()
            .uri("/api/elysia-unauth-settings")
            .peer_addr($peer.parse::<SocketAddr>().unwrap());
        if let Some(forwarded_for) = $forwarded_for {
            request = request.insert_header(("X-Forwarded-For", forwarded_for));
        }
        actix_test::call_service($app, request.to_request())
            .await
            .status()
    }};
}

#[sqlx::test(migrations = "./migrations")]
async fn limits_each_ip_separately(pool: PgPool) {
    let app = limited_app!(pool, limits(2, 0, 0));
    for _ in 0..2 {
        assert_eq!(
            status_from!(&app, "8.8.8.8:1", None::<&str>),
            StatusCode::OK
        );
    }
    let request = actix_test::TestRequest::get()
        .uri("/api/elysia-unauth-settings")
        .peer_addr("8.8.8.8:1".parse().unwrap())
        .to_request();
    let response = actix_test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("Retry-After"));
    // Images share the budget.
    let request = actix_test::TestRequest::get()
        .uri("/i/missing.png")
        .peer_addr("8.8.8.8:1".parse().unwrap())
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    // Another IP has its own budget, a neighbour in the same IPv6 /64 does not.
    assert_eq!(
        status_from!(&app, "1.1.1.1:1", None::<&str>),
        StatusCode::OK
    );
    assert_eq!(
        status_from!(&app, "[2001:db8::1]:1", None::<&str>),
        StatusCode::OK
    );
    assert_eq!(
        status_from!(&app, "[2001:db8::2]:1", None::<&str>),
        StatusCode::OK
    );
    assert_eq!(
        status_from!(&app, "[2001:db8::3]:1", None::<&str>),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn keys_on_the_forwarded_client_only_behind_a_trusted_proxy(pool: PgPool) {
    let app = limited_app!(pool, limits(1, 0, 0));
    // From the trusted proxy, the rightmost untrusted entry is the client: a
    // forged entry on its left does not buy a fresh budget.
    assert_eq!(
        status_from!(&app, "10.0.0.2:1", Some("6.6.6.6, 8.8.8.8")),
        StatusCode::OK
    );
    assert_eq!(
        status_from!(&app, "10.0.0.2:1", Some("7.7.7.7, 8.8.8.8, 10.0.0.3")),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        status_from!(&app, "10.0.0.2:1", Some("9.9.9.9")),
        StatusCode::OK
    );
    // Clients on the proxy's private network keep their own buckets.
    assert_eq!(
        status_from!(&app, "10.0.0.2:1", Some("192.168.1.5")),
        StatusCode::OK
    );
    assert_eq!(
        status_from!(&app, "10.0.0.2:1", Some("192.168.1.6")),
        StatusCode::OK
    );
    assert_eq!(
        status_from!(&app, "10.0.0.2:1", Some("192.168.1.6")),
        StatusCode::TOO_MANY_REQUESTS
    );
    // An untrusted peer cannot pick its key through the header.
    assert_eq!(
        status_from!(&app, "4.4.4.4:1", Some("5.5.5.5")),
        StatusCode::OK
    );
    assert_eq!(
        status_from!(&app, "4.4.4.4:1", Some("5.5.5.6")),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn limits_login_attempts_per_ip(pool: PgPool) {
    let (user, _) = test_user(&pool, "rate-limited-login").await;
    let app = limited_app!(pool, limits(0, 2, 0));
    let login_request = |password: &str| {
        actix_test::TestRequest::post()
            .uri("/api/auth/login")
            .peer_addr("8.8.8.8:1".parse().unwrap())
            .set_json(serde_json::json!({ "login": user.username, "password": password }))
            .to_request()
    };
    for _ in 0..2 {
        let response = actix_test::call_service(&app, login_request("wrong-password")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = actix_test::call_service(&app, login_request("long-test-password")).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrations = "./migrations")]
async fn limits_each_user_across_ips(pool: PgPool) {
    let (_, token) = test_user(&pool, "rate-limited-user").await;
    let (_, other_token) = test_user(&pool, "other-user").await;
    let app = limited_app!(pool, limits(0, 0, 1));
    let me_request = |token: &str, peer: &str| {
        actix_test::TestRequest::get()
            .uri("/api/auth/me")
            .peer_addr(peer.parse().unwrap())
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };
    let app = &app;
    let status = |request| async move { actix_test::call_service(app, request).await.status() };
    assert_eq!(
        status(me_request(&token, "8.8.8.8:1")).await,
        StatusCode::OK
    );
    assert_eq!(
        status(me_request(&token, "1.1.1.1:1")).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        status(me_request(&other_token, "8.8.8.8:1")).await,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_api_keys_spend_the_ip_budget_then_block_it(pool: PgPool) {
    let (user, _) = test_user(&pool, "api-key-owner").await;
    let key = auth_service::generate_api_key();
    api_keys::create(
        &pool,
        user.id,
        "valid",
        ApiKeyScope::Full,
        &key.token_hash,
        &key.token_prefix,
        None,
    )
    .await
    .unwrap();
    let app = limited_app!(pool, limits(1, 0, 0));
    let me = |token: &str| {
        actix_test::TestRequest::get()
            .uri("/api/auth/me")
            .peer_addr("8.8.8.8:1".parse().unwrap())
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };
    let app = &app;
    let status = |request| async move { actix_test::call_service(app, request).await.status() };
    // Successes cost nothing.
    for _ in 0..3 {
        assert_eq!(status(me(&key.token)).await, StatusCode::OK);
    }
    // Neither do requests without a token.
    for _ in 0..3 {
        let request = actix_test::TestRequest::get()
            .uri("/api/auth/me")
            .peer_addr("8.8.8.8:1".parse().unwrap())
            .to_request();
        assert_eq!(status(request).await, StatusCode::UNAUTHORIZED);
    }
    // The first failure spends the budget, the second blocks the IP...
    assert_eq!(status(me("elysia_wrong")).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        status(me("elysia_wrong")).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // ...so even a right key is refused before being looked up.
    assert_eq!(status(me(&key.token)).await, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrations = "./migrations")]
async fn limits_password_changes_per_ip(pool: PgPool) {
    let (_, token) = test_user(&pool, "rate-limited-password").await;
    let app = limited_app!(pool, limits(0, 1, 0));
    let change = || {
        actix_test::TestRequest::put()
            .uri("/api/auth/password")
            .peer_addr("8.8.8.8:1".parse().unwrap())
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(serde_json::json!({
                "current_password": "wrong-password",
                "new_password": "another-long-password",
            }))
            .to_request()
    };
    assert_eq!(
        actix_test::call_service(&app, change()).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        actix_test::call_service(&app, change()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}
