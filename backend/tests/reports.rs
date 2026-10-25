//! Reports: users flag images, admins work through the moderation queue.

mod common;

use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use serde_json::{Value, json};
use sqlx::PgPool;

use elysia::repository::users;
use elysia::routes;
use elysia::services::auth as auth_service;

#[sqlx::test(migrations = "./migrations")]
async fn reports_reach_the_moderation_queue_until_dismissed(pool: PgPool) {
    let (uploader, _) = common::test_user(&pool, "uploader").await;
    let (_, first_token) = common::test_user(&pool, "first").await;
    let (_, second_token) = common::test_user(&pool, "second").await;
    let admin_id = users::find_id_by_username(&pool, "admin")
        .await
        .unwrap()
        .unwrap();
    let admin_token =
        auth_service::mint_jwt(&admin_id, &common::test_config().auth.jwt_secret).unwrap();
    common::seed_files(&pool, uploader.id, "img", 2, 10).await;
    let (storage, _dir) = common::temp_storage();
    // Restoring moves the bytes back from the trash.
    let image_path = storage.path_for("img000000", "png");
    std::fs::create_dir_all(image_path.parent().unwrap()).unwrap();
    std::fs::write(image_path, b"bytes").unwrap();
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(common::test_config()))
            .app_data(Data::new(pool.clone()))
            .app_data(Data::new(storage))
            .app_data(Data::new(std::sync::RwLock::new(common::test_settings())))
            .configure(routes::init),
    )
    .await;
    let call = |request: actix_test::TestRequest, token: &str| {
        request
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };
    let report = |id: &str, reason: &str, token: &str| {
        call(
            actix_test::TestRequest::post()
                .uri(&format!("/api/i/{id}/report"))
                .set_json(json!({ "reason": reason })),
            token,
        )
    };

    let too_long = "a".repeat(501);
    for (id, reason, token, status) in [
        ("img000000", " ", &first_token, StatusCode::BAD_REQUEST),
        (
            "img000000",
            too_long.as_str(),
            &first_token,
            StatusCode::BAD_REQUEST,
        ),
        ("unknown", "spam", &first_token, StatusCode::NOT_FOUND),
        ("img000000", "spam", &first_token, StatusCode::NO_CONTENT),
        // Reporting again adds a report.
        ("img000000", "gore", &first_token, StatusCode::NO_CONTENT),
        ("img000001", "spam", &first_token, StatusCode::NO_CONTENT),
        ("img000001", "nsfw", &second_token, StatusCode::NO_CONTENT),
        (
            "img000001",
            "still nsfw",
            &second_token,
            StatusCode::NO_CONTENT,
        ),
    ] {
        let response = actix_test::call_service(&app, report(id, reason, token)).await;
        assert_eq!(response.status(), status);
    }

    let queue = |page: u32| {
        call(
            actix_test::TestRequest::get().uri(&format!("/api/reports?page={page}&page_size=1")),
            &admin_token,
        )
    };
    let post_ids = |uri: &str, id: &str, token: &str| {
        call(
            actix_test::TestRequest::post()
                .uri(uri)
                .set_json(json!({ "ids": [id] })),
            token,
        )
    };
    for request in [
        call(
            actix_test::TestRequest::get().uri("/api/reports?page=1&page_size=1"),
            &first_token,
        ),
        post_ids("/api/reports/dismiss", "img000001", &first_token),
    ] {
        let response = actix_test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    // The most reported image first, one image per page with all its reports.
    let reported: Value = actix_test::call_and_read_body_json(&app, queue(1)).await;
    assert_eq!(reported["total_items"], 2);
    assert_eq!(reported["results"][0]["image"]["id"], "img000001");
    assert_eq!(
        reported["results"][0]["reports"].as_array().unwrap().len(),
        3
    );
    let reported: Value = actix_test::call_and_read_body_json(&app, queue(2)).await;
    let reasons: Vec<&str> = reported["results"][0]["reports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|report| {
            assert_eq!(report["reporter_username"], "first");
            report["reason"].as_str().unwrap()
        })
        .collect();
    assert_eq!(reasons, ["spam", "gore"]);

    // Trashing hides an image from the queue, restoring brings it back,
    // dismissing drops its reports.
    for (uri, id, total) in [
        ("/api/images/trash", "img000000", 1),
        ("/api/images/trash/restore", "img000000", 2),
        ("/api/reports/dismiss", "img000001", 1),
    ] {
        let response = actix_test::call_service(&app, post_ids(uri, id, &admin_token)).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let reported: Value = actix_test::call_and_read_body_json(&app, queue(1)).await;
        assert_eq!(reported["total_items"], total);
    }
}
