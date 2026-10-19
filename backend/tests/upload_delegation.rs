//! The SSO provider uploading on behalf of its users with its client
//! credentials.

mod common;

use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use base64::{Engine, engine::general_purpose::STANDARD};
use sqlx::PgPool;

use elysia::repository::{files, users};
use elysia::routes;

use common::*;

const ISSUER: &str = "http://arcadia.test/api/oauth";

fn basic(client_id: &str, client_secret: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{client_id}:{client_secret}"))
    )
}

fn delegation_headers(client_secret: &str, subject: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Authorization", basic("elysia-test-client", client_secret)),
        ("On-Behalf-Of-Sub", subject.into()),
        ("On-Behalf-Of-Username", "arcadia-bob".into()),
    ]
}

async fn delegated_upload(
    pool: &PgPool,
    headers: &[(&str, String)],
) -> (StatusCode, serde_json::Value) {
    upload_with_headers(
        pool.clone(),
        sso_config(ISSUER),
        test_settings(),
        noisy_png(8, 8),
        "",
        headers,
    )
    .await
}

#[sqlx::test(migrations = "./migrations")]
async fn delegated_upload_is_attributed_to_the_sso_user(pool: PgPool) {
    let (status, body) =
        delegated_upload(&pool, &delegation_headers("elysia-test-secret", "42")).await;
    assert_eq!(status, StatusCode::OK);
    // Absolute, so the link works from the provider's site.
    assert!(
        body["url"]
            .as_str()
            .unwrap()
            .starts_with("http://elysia.test/i/")
    );

    // Same row an SSO login of subject 42 would use.
    let row = users::upsert_sso_user(&pool, ISSUER, "42", "ignored", None)
        .await
        .unwrap();
    assert_eq!(row.username, "arcadia-bob");
    let file = files::find(&pool, body["id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(file.uploaded_by_user_id, row.id);
}

#[sqlx::test(migrations = "./migrations")]
async fn delegation_is_refused_without_valid_credentials(pool: PgPool) {
    let (status, _) = delegated_upload(&pool, &delegation_headers("wrong-secret", "42")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = delegated_upload(&pool, &delegation_headers("elysia-test-secret", "")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Without SSO configured there are no client credentials to match.
    let (status, _) = upload_with_headers(
        pool.clone(),
        test_config(),
        test_settings(),
        noisy_png(8, 8),
        "",
        &delegation_headers("elysia-test-secret", "42"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn delegation_respects_local_ban(pool: PgPool) {
    let row = users::upsert_sso_user(&pool, ISSUER, "42", "arcadia-bob", None)
        .await
        .unwrap();
    users::set_banned(&pool, row.id, true).await.unwrap();
    let (status, _) =
        delegated_upload(&pool, &delegation_headers("elysia-test-secret", "42")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn client_credentials_open_nothing_but_uploads(pool: PgPool) {
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(sso_config(ISSUER)))
            .app_data(Data::new(pool))
            .configure(routes::init),
    )
    .await;
    let mut request = actix_test::TestRequest::get().uri("/api/auth/me");
    for (name, value) in delegation_headers("elysia-test-secret", "42") {
        request = request.insert_header((name, value));
    }
    let response = actix_test::call_service(&app, request.to_request()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
