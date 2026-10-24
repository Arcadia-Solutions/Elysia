//! Albums: ownership, images from other uploaders, listing and the image page.

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
async fn albums_are_public_and_hold_any_image(pool: PgPool) {
    let (owner, owner_token) = common::test_user(&pool, "owner").await;
    let (uploader, other_token) = common::test_user(&pool, "uploader").await;
    let admin_id = users::find_id_by_username(&pool, "admin")
        .await
        .unwrap()
        .unwrap();
    let admin_token =
        auth_service::mint_jwt(&admin_id, &common::test_config().auth.jwt_secret).unwrap();
    common::seed_files(&pool, owner.id, "mine", 1, 10).await;
    common::seed_files(&pool, uploader.id, "theirs", 1, 10).await;
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

    let create = |name: &str, token: &str| {
        call(
            actix_test::TestRequest::post()
                .uri("/api/albums")
                .set_json(json!({ "name": name })),
            token,
        )
    };
    let album: Value =
        actix_test::call_and_read_body_json(&app, create(" Trips ", &owner_token)).await;
    assert_eq!(album["name"], "Trips");
    let album_id = album["id"].as_str().unwrap().to_string();
    for (name, status) in [
        ("Trips", StatusCode::CONFLICT),
        (" ", StatusCode::BAD_REQUEST),
    ] {
        let response = actix_test::call_service(&app, create(name, &owner_token)).await;
        assert_eq!(response.status(), status);
    }
    // Another user may reuse the name.
    let response = actix_test::call_service(&app, create("Trips", &other_token)).await;
    assert_eq!(response.status(), StatusCode::OK);

    // The owner adds someone else's image; unknown ids are skipped.
    let add = |token: &str| {
        call(
            actix_test::TestRequest::post()
                .uri(&format!("/api/albums/{album_id}/images"))
                .set_json(json!({ "ids": ["mine000000", "theirs000000", "unknown"] })),
            token,
        )
    };
    let response = actix_test::call_service(&app, add(&owner_token)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    // Other users may not change it.
    let response = actix_test::call_service(&app, add(&other_token)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let album_images = |token: &str| {
        call(
            actix_test::TestRequest::get().uri(&format!(
                "/api/images?page=1&page_size=10&sort_by=created_at&order=desc&status=available&album_id={album_id}"
            )),
            token,
        )
    };
    // But anyone may view it.
    let page: Value = actix_test::call_and_read_body_json(&app, album_images(&other_token)).await;
    assert_eq!(page["total_items"], 2);
    let get_album = |token: &str| {
        call(
            actix_test::TestRequest::get().uri(&format!("/api/albums/{album_id}")),
            token,
        )
    };
    let fetched: Value = actix_test::call_and_read_body_json(&app, get_album(&owner_token)).await;
    assert_eq!(fetched["mine"], true);
    let fetched: Value = actix_test::call_and_read_body_json(&app, get_album(&other_token)).await;
    assert_eq!(fetched["mine"], false);

    // The image page shows every album to everyone, flagging the caller's own.
    let albums_of = |token: &str| {
        call(
            actix_test::TestRequest::get().uri("/api/i/theirs000000"),
            token,
        )
    };
    let metadata: Value = actix_test::call_and_read_body_json(&app, albums_of(&owner_token)).await;
    assert_eq!(metadata["albums"][0]["id"], album_id.as_str());
    assert_eq!(metadata["albums"][0]["mine"], true);
    let metadata: Value = actix_test::call_and_read_body_json(&app, albums_of(&other_token)).await;
    assert_eq!(metadata["albums"][0]["id"], album_id.as_str());
    assert_eq!(metadata["albums"][0]["mine"], false);

    // An admin removes an image from it, then lists everyone's albums.
    let response = actix_test::call_service(
        &app,
        call(
            actix_test::TestRequest::post()
                .uri(&format!("/api/albums/{album_id}/images/remove"))
                .set_json(json!({ "ids": ["theirs000000"] })),
            &admin_token,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let list = |query: &str, token: &str| {
        call(
            actix_test::TestRequest::get().uri(&format!(
                "/api/albums?page=1&page_size=10&sort_by=image_count&order=desc{query}"
            )),
            token,
        )
    };
    let albums: Value = actix_test::call_and_read_body_json(&app, list("", &admin_token)).await;
    assert_eq!(albums["total_items"], 2);
    assert_eq!(albums["results"][0]["image_count"], 1);
    let albums: Value =
        actix_test::call_and_read_body_json(&app, list("&mine=true", &admin_token)).await;
    assert_eq!(albums["total_items"], 0);
    let albums: Value = actix_test::call_and_read_body_json(&app, list("", &owner_token)).await;
    assert_eq!(albums["total_items"], 1);
    let response = actix_test::call_service(&app, list("&username=uploader", &owner_token)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Deleting the album keeps its images.
    let response = actix_test::call_service(
        &app,
        call(
            actix_test::TestRequest::delete().uri(&format!("/api/albums/{album_id}")),
            &owner_token,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = actix_test::call_service(&app, get_album(&owner_token)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = actix_test::call_service(&app, albums_of(&owner_token)).await;
    assert_eq!(response.status(), StatusCode::OK);
}
