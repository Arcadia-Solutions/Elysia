//! API keys: scopes, upload tracking and revocation.

mod common;

use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use serde_json::json;
use sqlx::PgPool;

use elysia::repository::api_keys::{self, ApiKeyScope};
use elysia::repository::users;
use elysia::routes;
use elysia::services::auth as auth_service;

#[sqlx::test(migrations = "./migrations")]
async fn api_keys_are_scoped_tracked_and_revoked(pool: PgPool) {
    let (_, session_token) = common::test_user(&pool, "keyholder").await;
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(common::test_config()))
            .app_data(Data::new(pool.clone()))
            .app_data(Data::new(std::sync::RwLock::new(common::test_settings())))
            .configure(routes::init),
    )
    .await;
    let call = |request: actix_test::TestRequest, token: &str| {
        request
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };
    let create = |body: serde_json::Value, token: &str| {
        call(
            actix_test::TestRequest::post()
                .uri("/api/api-keys")
                .set_json(body),
            token,
        )
    };

    for invalid in [
        json!({"name": " ", "scope": "upload"}),
        json!({"name": "ok", "scope": "upload", "expires_at": "2000-01-01T00:00:00Z"}),
        json!({"name": "ok", "scope": "upload", "expires_at": "not a date"}),
    ] {
        let response = actix_test::call_service(&app, create(invalid, &session_token)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    let created: serde_json::Value = actix_test::call_and_read_body_json(
        &app,
        create(
            json!({"name": "ShareX", "scope": "upload", "expires_at": "9999-01-01T00:00:00Z"}),
            &session_token,
        ),
    )
    .await;
    let upload_token = created["token"].as_str().unwrap().to_string();
    let upload_key_id = created["api_key"]["id"].as_str().unwrap().to_string();
    assert!(created["api_key"]["expires_at"].is_string());
    let created: serde_json::Value = actix_test::call_and_read_body_json(
        &app,
        create(json!({"name": "script", "scope": "full"}), &session_token),
    )
    .await;
    let full_token = created["token"].as_str().unwrap().to_string();
    assert!(created["api_key"]["expires_at"].is_null());
    let response = actix_test::call_service(
        &app,
        create(json!({"name": "script", "scope": "upload"}), &session_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Both scopes upload; only the full one reaches other routes, and no key manages keys.
    for (width, token) in [(1, &upload_token), (2, &full_token)] {
        let (status, _) = common::upload_with_token(
            pool.clone(),
            common::test_settings(),
            common::real_png(width, 1),
            "",
            token,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    for (token, expected) in [
        (&upload_token, StatusCode::FORBIDDEN),
        (&full_token, StatusCode::OK),
    ] {
        let response = actix_test::call_service(
            &app,
            call(actix_test::TestRequest::get().uri("/api/auth/me"), token),
        )
        .await;
        assert_eq!(response.status(), expected);
    }
    let response = actix_test::call_service(
        &app,
        create(json!({"name": "escalation", "scope": "full"}), &full_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Revoked keys are refused but stay listed with their uploads.
    let revoke = |token: &str| {
        call(
            actix_test::TestRequest::post().uri(&format!("/api/api-keys/{upload_key_id}/revoke")),
            token,
        )
    };
    let response = actix_test::call_service(&app, revoke(&session_token)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = actix_test::call_service(&app, revoke(&session_token)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let (status, _) = common::upload_with_token(
        pool.clone(),
        common::test_settings(),
        common::real_png(3, 3),
        "",
        &upload_token,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let listed: serde_json::Value = actix_test::call_and_read_body_json(
        &app,
        call(
            actix_test::TestRequest::get().uri("/api/api-keys"),
            &session_token,
        ),
    )
    .await;
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.len(), 2);
    let key_named = |name: &str| listed.iter().find(|key| key["name"] == name).unwrap();
    let (full_key, upload_key) = (key_named("script"), key_named("ShareX"));
    assert_eq!(upload_key["active"], false);
    assert!(upload_key["revoked_at"].is_string());
    assert_eq!(full_key["active"], true);
    for key in [full_key, upload_key] {
        assert_eq!(key["uploads"], 1);
        assert!(key["last_used_at"].is_string());
    }

    // Revoking a key frees its name.
    let response = actix_test::call_service(
        &app,
        create(json!({"name": "ShareX", "scope": "upload"}), &session_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_keys_behave_like_revoked_ones(pool: PgPool) {
    let (user, session_token) = common::test_user(&pool, "expired").await;
    let generated = auth_service::generate_api_key();
    let expired = api_keys::create(
        &pool,
        user.id,
        "old",
        ApiKeyScope::Full,
        &generated.token_hash,
        &generated.token_prefix,
        Some("2000-01-01T00:00:00Z"),
    )
    .await
    .unwrap();
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(common::test_config()))
            .app_data(Data::new(pool.clone()))
            .configure(routes::init),
    )
    .await;

    let (status, _) = common::upload_with_token(
        pool.clone(),
        common::test_settings(),
        common::real_png(1, 1),
        "",
        &generated.token,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let request = actix_test::TestRequest::post()
        .uri(&format!("/api/api-keys/{}/revoke", expired.id))
        .insert_header(("Authorization", format!("Bearer {session_token}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::NOT_FOUND
    );
    let request = actix_test::TestRequest::post()
        .uri("/api/api-keys")
        .insert_header(("Authorization", format!("Bearer {session_token}")))
        .set_json(json!({"name": "old", "scope": "upload"}))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_keys_reach_neither_admin_routes_nor_password_change(pool: PgPool) {
    let config = common::test_config();
    let admin = users::find_by_username_or_email(&pool, "admin")
        .await
        .unwrap()
        .unwrap();
    let session_token = auth_service::mint_jwt(&admin.id, &config.auth.jwt_secret).unwrap();
    let generated = auth_service::generate_api_key();
    api_keys::create(
        &pool,
        admin.id,
        "admin script",
        ApiKeyScope::Full,
        &generated.token_hash,
        &generated.token_prefix,
        None,
    )
    .await
    .unwrap();
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(config))
            .app_data(Data::new(pool.clone()))
            .app_data(Data::new(std::sync::RwLock::new(common::test_settings())))
            .configure(routes::init),
    )
    .await;
    let list_users_request = |token: &str| {
        actix_test::TestRequest::get()
            .uri("/api/users?page=1&page_size=50&sort_by=username&order=asc")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };

    for (token, expected) in [
        (&session_token, StatusCode::OK),
        (&generated.token, StatusCode::FORBIDDEN),
    ] {
        let response = actix_test::call_service(&app, list_users_request(token)).await;
        assert_eq!(response.status(), expected);
    }
    let request = actix_test::TestRequest::put()
        .uri("/api/auth/password")
        .insert_header(("Authorization", format!("Bearer {}", generated.token)))
        .set_json(json!({"current_password": "admin", "new_password": "another-long-password"}))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::FORBIDDEN
    );
}
