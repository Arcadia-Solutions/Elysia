// Its own test binary: the outbound proxy is process-wide and set only once.

#[expect(dead_code, reason = "each binary uses only part of the shared helpers")]
mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{LazyLock, Mutex};

use actix_web::http::StatusCode;
use sqlx::PgPool;

use elysia::config::{http_client_builder, set_outbound_proxy};
use elysia::repository::api_keys::{self, ApiKeyScope};
use elysia::services::auth as auth_service;
use elysia::services::files;
use elysia::settings::UrlUploads;

use common::*;

// `.invalid` never resolves, so requests to it only succeed through the proxy.
const TARGET: &str = "http://elysia-proxy-test.invalid/image.png";
const REHOST_TARGET: &str = "http://elysia-proxy-test.invalid/rehosted.png";

/// Request lines received by the proxy, each naming the absolute target URL.
/// The first use starts the proxy on a loopback port, answering every request
/// with a PNG, and routes outbound requests through it.
static PROXY_REQUEST_LINES: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    set_outbound_proxy(&format!("http://{}", listener.local_addr().unwrap()));
    let image = real_png(2, 2);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0u8; 1024];
            let length = stream.read(&mut request).unwrap_or(0);
            let request = String::from_utf8_lossy(&request[..length]);
            let request_line = request.lines().next().unwrap_or_default().to_string();
            PROXY_REQUEST_LINES.lock().unwrap().push(request_line);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                image.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&image);
        }
    });
    Mutex::new(Vec::new())
});

/// How many requests for `target` the proxy received.
fn proxy_requests_for(target: &str) -> usize {
    let expected = format!("GET {target} HTTP/1.1");
    PROXY_REQUEST_LINES
        .lock()
        .unwrap()
        .iter()
        .filter(|line| **line == expected)
        .count()
}

#[actix_web::test]
async fn outbound_requests_go_through_the_configured_proxy() {
    LazyLock::force(&PROXY_REQUEST_LINES);

    let plain_client = http_client_builder().build().unwrap();
    let response = plain_client.get(TARGET).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(proxy_requests_for(TARGET), 1);

    // The URL upload client, which otherwise only connects to public addresses.
    let response = files::http_client().get(TARGET).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(proxy_requests_for(TARGET), 2);
}

#[sqlx::test]
async fn sso_only_url_uploads_refuse_users_but_let_the_provider_rehost(pool: PgPool) {
    LazyLock::force(&PROXY_REQUEST_LINES);
    let mut settings = test_settings();
    settings.url_uploads = UrlUploads::SsoOnly;
    let (user, token) = test_user(&pool, "url-uploader").await;
    let api_key = auth_service::generate_api_key();
    api_keys::create(
        &pool,
        user.id,
        "script",
        ApiKeyScope::Upload,
        &api_key.token_hash,
        &api_key.token_prefix,
        None,
    )
    .await
    .unwrap();
    let rehost = async |headers: Vec<(&'static str, String)>| {
        rehost_status(
            pool.clone(),
            sso_config("http://arcadia.test/api/oauth"),
            settings.clone(),
            REHOST_TARGET,
            &headers,
        )
        .await
    };

    let session = vec![("Authorization", format!("Bearer {token}"))];
    assert_eq!(rehost(session).await, StatusCode::FORBIDDEN);
    let key = vec![("Authorization", format!("Bearer {}", api_key.token))];
    assert_eq!(rehost(key).await, StatusCode::FORBIDDEN);
    let provider = delegation_headers("elysia-test-secret", "42");
    assert_eq!(rehost(provider).await, StatusCode::OK);
    assert_eq!(proxy_requests_for(REHOST_TARGET), 1);
}
