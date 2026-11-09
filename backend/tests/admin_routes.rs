//! Every admin route refuses anonymous callers and regular users, and serves admins.

use crate::common;

use actix_web::App;
use actix_web::http::{Method, StatusCode};
use actix_web::test as actix_test;
use actix_web::web::Data;
use serde_json::{Value, json};
use sqlx::PgPool;

use elysia::repository::users;
use elysia::routes;
use elysia::services::auth as auth_service;

#[sqlx::test]
async fn admin_routes_require_an_admin(pool: PgPool) {
    let (user, user_token) = common::test_user(&pool, "regular").await;
    let admin_id = users::find_id_by_username(&pool, "admin")
        .await
        .unwrap()
        .unwrap();
    let admin_token = auth_service::start_session(
        &pool,
        &admin_id,
        false,
        &common::test_config().auth.jwt_secret,
    )
    .await
    .unwrap()
    .token;
    let (storage, _upload_dir) = common::temp_storage();
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(common::test_config()))
            .app_data(Data::new(pool.clone()))
            .app_data(Data::new(storage))
            .app_data(Data::new(std::sync::RwLock::new(common::test_settings())))
            .configure(routes::init),
    )
    .await;

    let pagination = "page=1&page_size=10";
    let quota_uri = format!("/api/users/{}/storage-quota", user.id);
    let chevereto_import = json!({
        "chevereto_database_url": "mysql://chevereto:password@localhost:1/chevereto",
        "chevereto_public_url": "http://chevereto.test",
        "fallback_username": "chevereto-import",
        "batch_size": 1,
        "concurrency": 1,
        "start": "resume",
    });
    // Routes that start background jobs are only checked for refusal: running
    // them here would leak into the global job state other tests rely on.
    let routes: [(Method, &str, Value, bool); 18] = [
        (
            Method::GET,
            "/api/stats/overview?period=last_week&interval=day",
            Value::Null,
            true,
        ),
        (
            Method::PUT,
            &quota_uri,
            json!({ "storage_quota_bytes": 10 }),
            true,
        ),
        (Method::POST, "/api/images/trash/empty", Value::Null, true),
        (
            Method::POST,
            "/api/maintenance/views/reset",
            json!({ "kind": "all" }),
            true,
        ),
        (
            Method::POST,
            "/api/maintenance/sizes/recalculate",
            Value::Null,
            true,
        ),
        (
            Method::GET,
            "/api/maintenance/unreferenced-images/progress",
            Value::Null,
            true,
        ),
        (
            Method::GET,
            &format!(
                "/api/maintenance/unreferenced-images?{pagination}&sort_by=created_at&order=desc"
            ),
            Value::Null,
            true,
        ),
        (
            Method::POST,
            "/api/maintenance/unreferenced-images/scan",
            json!({ "database_url": "postgres://localhost:1/none", "hosts": [] }),
            false,
        ),
        (
            Method::POST,
            "/api/maintenance/unreferenced-images/trash",
            Value::Null,
            false,
        ),
        (
            Method::GET,
            "/api/maintenance/similar-images/progress",
            Value::Null,
            true,
        ),
        (
            Method::GET,
            &format!("/api/maintenance/similar-images?{pagination}"),
            Value::Null,
            true,
        ),
        (
            Method::POST,
            "/api/maintenance/similar-images/scan",
            json!({ "max_distance": 0 }),
            false,
        ),
        (
            Method::GET,
            "/api/maintenance/chevereto-import/progress",
            Value::Null,
            true,
        ),
        (
            Method::GET,
            &format!("/api/maintenance/chevereto-import/failures?{pagination}"),
            Value::Null,
            true,
        ),
        (
            Method::POST,
            "/api/maintenance/chevereto-import",
            chevereto_import,
            false,
        ),
        (
            Method::POST,
            "/api/maintenance/chevereto-import/stop",
            Value::Null,
            false,
        ),
        (Method::GET, "/api/fixtures", Value::Null, true),
        (
            Method::POST,
            "/api/fixtures",
            json!({ "image_count": 1, "user_count": 1, "history_days": 1, "password": "long-test-password" }),
            false,
        ),
    ];
    let request = |method: &Method, uri: &str, body: &Value, token: Option<&str>| {
        let mut request = actix_test::TestRequest::default()
            .method(method.clone())
            .uri(uri);
        if !body.is_null() {
            request = request.set_json(body);
        }
        if let Some(token) = token {
            request = request.insert_header(("Authorization", format!("Bearer {token}")));
        }
        request.to_request()
    };

    for (method, uri, body, admin_may_call) in &routes {
        for (token, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some(user_token.as_str()), StatusCode::FORBIDDEN),
        ] {
            let response = actix_test::call_service(&app, request(method, uri, body, token)).await;
            assert_eq!(response.status(), expected, "{method} {uri}");
        }
        if *admin_may_call {
            let response =
                actix_test::call_service(&app, request(method, uri, body, Some(&admin_token)))
                    .await;
            assert!(
                response.status().is_success(),
                "{method} {uri}: {}",
                response.status()
            );
        }
    }
}
