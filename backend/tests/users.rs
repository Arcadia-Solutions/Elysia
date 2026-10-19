//! Admin user management: listing, profiles, bans and the browse username filter.

mod common;

use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use actix_web::{App, web};
use sqlx::PgPool;

use elysia::handlers::{get_user, images, list_users, set_user_banned};
use elysia::repository::users;
use elysia::services::auth as auth_service;

#[sqlx::test(migrations = "./migrations")]
async fn admin_lists_and_bans_users(pool: PgPool) {
    let config = common::test_config();
    let admin = users::find_by_username_or_email(&pool, "admin")
        .await
        .unwrap()
        .unwrap();
    let admin_token = auth_service::mint_jwt(&admin.id, &config.auth.jwt_secret).unwrap();
    let (local, local_token) = common::test_user(&pool, "local").await;
    let sso = users::upsert_sso_user(&pool, "https://issuer.test", "subject", "sso", None)
        .await
        .unwrap();
    common::seed_files(&pool, local.id, "local", 2, 1).await;
    common::seed_files(&pool, sso.id, "sso", 1, 1).await;

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(config))
            .app_data(Data::new(pool.clone()))
            .route("/api/users", web::get().to(list_users))
            .route("/api/users/{id}", web::get().to(get_user))
            .route("/api/users/{id}/banned", web::put().to(set_user_banned))
            .route("/api/images", web::get().to(images)),
    )
    .await;
    let call = |method: actix_test::TestRequest, uri: String, token: &str| {
        method
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };

    // Sorted by upload count, and searched by username or email ignoring case.
    for (query, expected) in [
        (
            "page=1&sort_by=uploads&order=desc",
            vec!["local", "sso", "admin"],
        ),
        (
            "page=1&sort_by=username&order=asc&search=L",
            vec!["admin", "local"],
        ),
        (
            "page=1&sort_by=username&order=asc&search=EXAMPLE.com",
            vec!["admin", "local"],
        ),
    ] {
        let response = actix_test::call_service(
            &app,
            call(
                actix_test::TestRequest::get(),
                format!("/api/users?{query}"),
                &admin_token,
            ),
        )
        .await;
        let listed: serde_json::Value = actix_test::read_body_json(response).await;
        assert_eq!(listed["total"], expected.len(), "{query}");
        let usernames: Vec<&str> = listed["users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|user| user["username"].as_str().unwrap())
            .collect();
        assert_eq!(usernames, expected, "{query}");
        if query.contains("sort_by=uploads") {
            assert_eq!(listed["users"][0]["uploads"], 2);
        }
    }

    // Authenticated requests stamp `last_seen`.
    let summary = |id| users::find_summary(&pool, id);
    assert!(
        summary(admin.id)
            .await
            .unwrap()
            .unwrap()
            .last_seen
            .is_some()
    );
    assert!(
        summary(local.id)
            .await
            .unwrap()
            .unwrap()
            .last_seen
            .is_none()
    );

    // Local and SSO users are banned the same way; admins cannot ban themselves.
    for (id, expected) in [
        (local.id, StatusCode::NO_CONTENT),
        (sso.id, StatusCode::NO_CONTENT),
        (admin.id, StatusCode::BAD_REQUEST),
        (uuid::Uuid::new_v4(), StatusCode::NOT_FOUND),
    ] {
        let request =
            actix_test::TestRequest::put().set_json(serde_json::json!({ "banned": true }));
        let response = actix_test::call_service(
            &app,
            call(request, format!("/api/users/{id}/banned"), &admin_token),
        )
        .await;
        assert_eq!(response.status(), expected);
    }
    let response = actix_test::call_service(
        &app,
        call(
            actix_test::TestRequest::get(),
            format!("/api/users/{}", sso.id),
            &admin_token,
        ),
    )
    .await;
    let profile: serde_json::Value = actix_test::read_body_json(response).await;
    assert_eq!(
        (profile["banned"].as_bool(), profile["provider"].as_str()),
        (Some(true), Some("sso"))
    );

    // The username filter shows one user's uploads, to admins only.
    let browse = |username: &str| {
        format!(
            "/api/images?page=1&sort_by=created_at&order=desc&status=available&username={username}"
        )
    };
    for (username, total) in [("local", 2), ("sso", 1), ("nobody", 0)] {
        let response = actix_test::call_service(
            &app,
            call(
                actix_test::TestRequest::get(),
                browse(username),
                &admin_token,
            ),
        )
        .await;
        let page: serde_json::Value = actix_test::read_body_json(response).await;
        assert_eq!(page["total"], total, "{username}");
    }

    // A banned user's session is refused; a regular user gets no admin routes.
    let response = actix_test::call_service(
        &app,
        call(actix_test::TestRequest::get(), browse("sso"), &local_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    users::set_banned(&pool, local.id, false).await.unwrap();
    for uri in [
        "/api/users?page=1&sort_by=username&order=asc".to_string(),
        browse("sso"),
    ] {
        let response = actix_test::call_service(
            &app,
            call(actix_test::TestRequest::get(), uri, &local_token),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
