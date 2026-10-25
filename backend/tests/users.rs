//! Admin user management: listing, profiles, bans, admin rights and the browse username filter.

mod common;

use actix_web::App;
use actix_web::dev::ServiceResponse;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use sqlx::PgPool;
use uuid::Uuid;

use elysia::repository::users::AuthUserRow;
use elysia::repository::{MAX_PAGE_SIZE, users};
use elysia::routes;
use elysia::services::auth as auth_service;

const USERS_URI: &str = "/api/users?page=1&page_size=50&sort_by=username&order=asc";

/// The seeded admin, a local user with 2 uploads and an SSO user with 1.
struct SeededUsers {
    admin_id: Uuid,
    admin_token: String,
    local: AuthUserRow,
    local_token: String,
    sso: AuthUserRow,
}

async fn seed_users(pool: &PgPool) -> SeededUsers {
    let admin = users::find_by_username_or_email(pool, "admin")
        .await
        .unwrap()
        .unwrap();
    let admin_token =
        auth_service::mint_jwt(&admin.id, &common::test_config().auth.jwt_secret).unwrap();
    let (local, local_token) = common::test_user(pool, "local").await;
    let sso = users::upsert_sso_user(pool, "https://issuer.test", "subject", "sso", None, false)
        .await
        .unwrap();
    common::seed_files(pool, local.id, "local", 2, 1).await;
    common::seed_files(pool, sso.id, "sso", 1, 1).await;
    SeededUsers {
        admin_id: admin.id,
        admin_token,
        local,
        local_token,
        sso,
    }
}

/// Sends one authenticated request to a fresh app.
async fn send(
    pool: &PgPool,
    request: actix_test::TestRequest,
    uri: &str,
    token: &str,
) -> ServiceResponse {
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(common::test_config()))
            .app_data(Data::new(pool.clone()))
            .app_data(Data::new(std::sync::RwLock::new(common::test_settings())))
            .configure(routes::init),
    )
    .await;
    let request = request
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    actix_test::call_service(&app, request).await
}

fn browse_uri(username: &str) -> String {
    format!(
        "/api/images?page=1&page_size=50&sort_by=created_at&order=desc&status=available&username={username}"
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn lists_and_searches_users(pool: PgPool) {
    let seeded = seed_users(&pool).await;

    // Sorted by upload count, and searched by username or email ignoring case.
    for (query, expected) in [
        (
            "page=1&page_size=50&sort_by=uploads&order=desc",
            vec!["local", "sso", "admin"],
        ),
        (
            "page=1&page_size=50&sort_by=username&order=asc&search=L",
            vec!["admin", "local"],
        ),
        (
            "page=1&page_size=50&sort_by=username&order=asc&search=EXAMPLE.com",
            vec!["admin", "local"],
        ),
    ] {
        let response = send(
            &pool,
            actix_test::TestRequest::get(),
            &format!("/api/users?{query}"),
            &seeded.admin_token,
        )
        .await;
        let listed: serde_json::Value = actix_test::read_body_json(response).await;
        assert_eq!(listed["total_items"], expected.len(), "{query}");
        let usernames: Vec<&str> = listed["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|user| user["username"].as_str().unwrap())
            .collect();
        assert_eq!(usernames, expected, "{query}");
        if query.contains("sort_by=uploads") {
            assert_eq!(listed["results"][0]["uploads"], 2);
        }
    }

    // Page sizes outside 1..=MAX_PAGE_SIZE are refused.
    for page_size in [0, MAX_PAGE_SIZE + 1] {
        let response = send(
            &pool,
            actix_test::TestRequest::get(),
            &format!("/api/users?page=1&page_size={page_size}&sort_by=username&order=asc"),
            &seeded.admin_token,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{page_size}");
    }

    // Authenticated requests stamp `last_seen`.
    let last_seen = async |id| {
        users::find_summary(&pool, id, 0)
            .await
            .unwrap()
            .unwrap()
            .last_seen
    };
    assert!(last_seen(seeded.admin_id).await.is_some());
    assert!(last_seen(seeded.local.id).await.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn bans_local_and_sso_users(pool: PgPool) {
    let seeded = seed_users(&pool).await;

    // Local and SSO users are banned the same way; admins cannot ban themselves.
    for (id, expected) in [
        (seeded.local.id, StatusCode::NO_CONTENT),
        (seeded.sso.id, StatusCode::NO_CONTENT),
        (seeded.admin_id, StatusCode::BAD_REQUEST),
        (Uuid::new_v4(), StatusCode::NOT_FOUND),
    ] {
        let request =
            actix_test::TestRequest::put().set_json(serde_json::json!({ "banned": true }));
        let response = send(
            &pool,
            request,
            &format!("/api/users/{id}/banned"),
            &seeded.admin_token,
        )
        .await;
        assert_eq!(response.status(), expected);
    }
    let response = send(
        &pool,
        actix_test::TestRequest::get(),
        &format!("/api/users/{}", seeded.sso.id),
        &seeded.admin_token,
    )
    .await;
    let profile: serde_json::Value = actix_test::read_body_json(response).await;
    assert_eq!(
        (profile["banned"].as_bool(), profile["provider"].as_str()),
        (Some(true), Some("sso"))
    );

    // A banned user's session is refused.
    let response = send(
        &pool,
        actix_test::TestRequest::get(),
        &browse_uri("sso"),
        &seeded.local_token,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_routes_are_refused_to_regular_users(pool: PgPool) {
    let seeded = seed_users(&pool).await;

    // The username filter shows one user's uploads, to admins only.
    for (username, total) in [("local", 2), ("sso", 1), ("nobody", 0)] {
        let response = send(
            &pool,
            actix_test::TestRequest::get(),
            &browse_uri(username),
            &seeded.admin_token,
        )
        .await;
        let page: serde_json::Value = actix_test::read_body_json(response).await;
        assert_eq!(page["total_items"], total, "{username}");
    }
    for uri in [USERS_URI.to_string(), browse_uri("sso")] {
        let response = send(
            &pool,
            actix_test::TestRequest::get(),
            &uri,
            &seeded.local_token,
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn grants_and_revokes_admin_rights(pool: PgPool) {
    let seeded = seed_users(&pool).await;
    let set_admin = |id: Uuid, is_admin: bool| {
        let request =
            actix_test::TestRequest::put().set_json(serde_json::json!({ "is_admin": is_admin }));
        let uri = format!("/api/users/{id}/admin");
        let pool = &pool;
        let token = &seeded.admin_token;
        async move { send(pool, request, &uri, token).await.status() }
    };
    let list_users_as_local = async || {
        send(
            &pool,
            actix_test::TestRequest::get(),
            USERS_URI,
            &seeded.local_token,
        )
        .await
        .status()
    };

    // Admins cannot change their own rights.
    assert_eq!(
        set_admin(seeded.admin_id, false).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(set_admin(Uuid::new_v4(), true).await, StatusCode::NOT_FOUND);

    // Changes apply to the user's existing session.
    assert_eq!(
        set_admin(seeded.local.id, true).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(list_users_as_local().await, StatusCode::OK);
    assert_eq!(
        set_admin(seeded.local.id, false).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(list_users_as_local().await, StatusCode::FORBIDDEN);
}
