use crate::common;

use actix_web::web::Data;
use sqlx::PgPool;

use elysia::error::Error;
use elysia::services::chevereto_import::*;
use elysia::services::files::StoredFile;

fn image(storage_mode: &str, name: &str, storage_url: Option<&str>) -> CheveretoImage {
    CheveretoImage {
        id: 61,
        name: name.to_string(),
        extension: "png".to_string(),
        storage_mode: storage_mode.to_string(),
        date_folder: "2024/05/03".to_string(),
        uploaded_at: None,
        original_filename: None,
        storage_url: storage_url.map(str::to_string),
        uploader_username: None,
    }
}

/// A stored Elysia image `<id>.webp`, with or without generated previews.
fn stored_file(id: &str, has_previews: bool) -> StoredFile {
    StoredFile {
        id: id.to_string(),
        ext: "webp".to_string(),
        existed: false,
        actions: None,
        has_medium: has_previews,
        has_thumbnail: has_previews,
    }
}

fn settings() -> LinkSettings {
    LinkSettings {
        public_url: "https://img.example.com/".to_string(),
        link_hosts: vec!["img.example.com".to_string(), "old.example.com".to_string()],
        elysia_public_url: "https://elysia.example/".to_string(),
        salt: "testsalt".to_string(),
        id_padding: 0,
    }
}

#[test]
fn encode_id_matches_php_vectors() {
    assert_eq!(encode_id(1, "", 0), "k");
    assert_eq!(encode_id(1, "testsalt", 0), "n");
    assert_eq!(encode_id(61, "testsalt", 0), "e");
    assert_eq!(encode_id(62, "testsalt", 0), "nd");
    assert_eq!(encode_id(123456, "testsalt", 0), "jbz");
    assert_eq!(encode_id(123456, "testsalt", 3), "nxrk");
    assert_eq!(encode_id(999999999, "Zx9!salt", 0), "QNzKF3");
    // From a real Chevereto 4.5.7 instance (crypt_salt 03a8032f, id_padding 999).
    assert_eq!(encode_id(1, "03a8032f", 999), "4H");
    assert_eq!(encode_id(2, "03a8032f", 999), "DK");
    assert_eq!(encode_id(3, "03a8032f", 999), "sZ");
}

#[test]
fn fetch_url_per_storage_mode() {
    let settings = settings();
    assert_eq!(
        fetch_url(&image("datefolder", "abc", None), &settings).unwrap(),
        "https://img.example.com/images/2024/05/03/abc.png"
    );
    assert_eq!(
        fetch_url(&image("direct", "abc", None), &settings).unwrap(),
        "https://img.example.com/images/abc.png"
    );
    assert_eq!(
        fetch_url(
            &image("datefolder", "abc", Some("https://cdn.example.com/bucket")),
            &settings
        )
        .unwrap(),
        "https://cdn.example.com/bucket/2024/05/03/abc.png"
    );
    assert!(fetch_url(&image("path", "abc", None), &settings).is_none());
}

#[test]
fn link_rewrites_keys() {
    let rewrites = link_rewrites(
        &image("datefolder", "my.photo", None),
        &settings(),
        &stored_file("eid", true),
    )
    .unwrap();
    let image_link = "https://elysia.example/i/eid.webp".to_string();
    let medium_link = "https://elysia.example/m/eid.webp".to_string();
    let thumbnail_link = "https://elysia.example/t/eid.webp".to_string();
    let file = "img.example.com/images/2024/05/03/my.photo";
    assert_eq!(rewrites[0], (format!("{file}.png"), image_link));
    assert_eq!(rewrites[1], (format!("{file}.md.png"), medium_link));
    assert_eq!(rewrites[2], (format!("{file}.th.png"), thumbnail_link));
    assert_eq!(
        rewrites[7],
        (
            "old.example.com/image/e".to_string(),
            "https://elysia.example/i/eid".to_string()
        )
    );
    let stored = link_rewrites(
        &image("direct", "abc", Some("https://cdn.example.com/bucket")),
        &settings(),
        &stored_file("eid", false),
    )
    .unwrap();
    assert_eq!(stored[0].0, "cdn.example.com/bucket/abc.png");
    assert_eq!(stored[2].1, "https://elysia.example/t/eid.webp");
    assert!(
        link_rewrites(
            &image("path", "abc", None),
            &settings(),
            &stored_file("eid", true)
        )
        .is_none()
    );
}

fn test_pattern() -> String {
    link_pattern(
        &settings(),
        &["https://cdn.example.com/bucket".to_string()],
        &["png".to_string(), "jpg".to_string()],
    )
}

fn rewrite_test_rewrites() -> Vec<(String, String)> {
    let mut target = image("datefolder", "abc", None);
    target.id = 123456;
    link_rewrites(&target, &settings(), &stored_file("eid", true)).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn apply_link_rewrites_in_postgres(pool: sqlx::PgPool) {
    let mut connection = pool.acquire().await.unwrap();
    install_backend_functions(&mut connection).await.unwrap();
    let rewrites = rewrite_test_rewrites();
    let pattern = test_pattern();
    let cases = [
        (
            "https://img.example.com/images/2024/05/03/abc.png",
            "https://elysia.example/i/eid.webp",
        ),
        (
            "http://img.example.com/images/2024/05/03/abc.png",
            "https://elysia.example/i/eid.webp",
        ),
        (
            "https://img.example.com/images/2024/05/03/abc.th.png",
            "https://elysia.example/t/eid.webp",
        ),
        (
            "https://img.example.com/images/2024/05/03/abc.md.png",
            "https://elysia.example/m/eid.webp",
        ),
        (
            "https://img.example.com/image/my-slug.jbz",
            "https://elysia.example/i/eid",
        ),
        (
            "https://img.example.com/image/jbz",
            "https://elysia.example/i/eid",
        ),
        (
            "look: https://img.example.com/images/2024/05/03/abc.png.",
            "look: https://elysia.example/i/eid.webp.",
        ),
        (
            "https://old.example.com/image/jbz",
            "https://elysia.example/i/eid",
        ),
        (
            "[img]https://img.example.com/images/2024/05/03/abc.png[/img] and https://img.example.com/images/2024/05/03/other.png, ![x](http://img.example.com/image/a-b.c.jbz)",
            "[img]https://elysia.example/i/eid.webp[/img] and https://img.example.com/images/2024/05/03/other.png, ![x](https://elysia.example/i/eid)",
        ),
        (
            "https://img.example.com/images/2024/05/03/abc.pngx",
            "https://img.example.com/images/2024/05/03/abc.pngx",
        ),
        (
            "https://img.example.com/image/jbzq",
            "https://img.example.com/image/jbzq",
        ),
        (
            "https://example.org/images/2024/05/03/abc.png",
            "https://example.org/images/2024/05/03/abc.png",
        ),
    ];
    for (input, expected) in cases {
        let actual = apply_link_rewrites(&mut connection, input, &pattern, &rewrites)
            .await
            .unwrap();
        assert_eq!(actual, expected, "input: {input}");
    }
}

#[sqlx::test(migrations = "./migrations", fixtures("chevereto_links"))]
async fn rewrite_links_updates_text_like_columns(pool: sqlx::PgPool) {
    let mut connection = pool.acquire().await.unwrap();
    install_backend_functions(&mut connection).await.unwrap();
    let rewrites = rewrite_test_rewrites();
    let pattern = test_pattern();
    assert_eq!(count_link_rows(&mut connection, &pattern).await.unwrap(), 7);
    // Rows 1 to 4, 6 and 7, each counted once even with several changed columns.
    assert_eq!(
        rewrite_links(&mut connection, &pattern, &rewrites)
            .await
            .unwrap(),
        6
    );
    assert_eq!(
        rewrite_links(&mut connection, &pattern, &rewrites)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        rewrite_links(&mut connection, &pattern, &[]).await.unwrap(),
        0
    );
    // Row 8 links an image with no rewrite.
    assert_eq!(count_link_rows(&mut connection, &pattern).await.unwrap(), 1);
}

#[sqlx::test(
    migrations = "./migrations",
    fixtures("chevereto_links", "chevereto_overflow")
)]
async fn rewrite_links_errors_instead_of_truncating(pool: sqlx::PgPool) {
    let mut connection = pool.acquire().await.unwrap();
    install_backend_functions(&mut connection).await.unwrap();
    let mut target = image("datefolder", "abc", None);
    target.id = 123456;
    let long_id = "e".repeat(40);
    let rewrites = link_rewrites(&target, &settings(), &stored_file(&long_id, true)).unwrap();
    assert!(
        rewrite_links(&mut connection, &test_pattern(), &rewrites)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "./migrations", fixtures("backend_users"))]
async fn find_backend_user_uses_configured_columns(pool: sqlx::PgPool) {
    let mut connection = pool.acquire().await.unwrap();
    install_backend_functions(&mut connection).await.unwrap();
    let mut users = BackendUsers {
        table: "public.members".to_string(),
        username_column: "login".to_string(),
        subject_column: "member_id".to_string(),
        email_column: Some("mail".to_string()),
    };
    assert_eq!(
        find_backend_user(&mut connection, &users, "alice")
            .await
            .unwrap(),
        Some(("7".to_string(), Some("alice@example.com".to_string())))
    );
    assert_eq!(
        find_backend_user(&mut connection, &users, "bob")
            .await
            .unwrap(),
        None
    );
    users.email_column = None;
    assert_eq!(
        find_backend_user(&mut connection, &users, "alice")
            .await
            .unwrap(),
        Some(("7".to_string(), None))
    );
}

fn import_params(backend_users: Option<BackendUsers>) -> CheveretoImportParams {
    CheveretoImportParams {
        chevereto_database_url: "mysql://chevereto@localhost:not-a-port/chevereto".to_string(),
        chevereto_public_url: "https://img.example.com".to_string(),
        link_hosts: Vec::new(),
        backend_database_url: None,
        backend_users,
        fallback_username: "chevereto-import".to_string(),
        batch_size: 10,
        concurrency: 2,
        start: CheveretoImportStart::Resume,
        dry_run: false,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn import_checks_params_and_services_before_starting(pool: PgPool) {
    let (storage, _dir) = common::temp_storage();
    let storage = Data::new(storage);
    let config = common::test_config();
    let start = |params| {
        elysia::services::chevereto_import::start(
            pool.clone(),
            storage.clone(),
            &config,
            common::test_settings(),
            params,
        )
    };
    let users = BackendUsers {
        table: "users".to_string(),
        username_column: "username".to_string(),
        subject_column: "id".to_string(),
        email_column: None,
    };
    assert!(matches!(
        start(import_params(Some(users))).await,
        Err(Error::BadRequest(_))
    ));

    // An unreachable Chevereto database fails the request and starts nothing.
    let Err(Error::BadRequest(message)) = start(import_params(None)).await else {
        panic!("expected a bad request");
    };
    assert!(message.starts_with("chevereto database"));
    let progress = progress(&pool).await.unwrap();
    assert!(!progress.running);
    assert!(progress.started_at.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn retry_keeps_resume_point_and_clears_retried_failures(pool: PgPool) {
    use elysia::repository::chevereto_import as import_repo;
    import_repo::start(&pool, Some(0), 10, false).await.unwrap();
    import_repo::record_failure(&pool, 3, None, "first")
        .await
        .unwrap();
    import_repo::record_failure(&pool, 7, None, "first")
        .await
        .unwrap();
    import_repo::finish_chunk(&pool, Some(10), 0).await.unwrap();

    import_repo::start(&pool, None, 2, false).await.unwrap();
    assert_eq!(import_repo::failed_image_ids(&pool).await.unwrap(), [3, 7]);
    import_repo::record_imported(&pool, Some(3)).await.unwrap();
    import_repo::record_failure(&pool, 7, None, "again")
        .await
        .unwrap();
    import_repo::finish_chunk(&pool, None, 0).await.unwrap();
    let row = import_repo::get(&pool).await.unwrap();
    assert_eq!(
        (row.last_image_id, row.total, row.done, row.failed),
        (10, 2, 2, 1)
    );
    let failures = import_repo::failures_page(&pool, 0, 10).await.unwrap();
    assert_eq!(
        (failures[0].image_id, failures[0].error.as_str()),
        (7, "again")
    );

    import_repo::start(&pool, Some(5), 0, false).await.unwrap();
    assert!(
        import_repo::failed_image_ids(&pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn dry_run_keeps_resume_point_and_real_failures(pool: PgPool) {
    use elysia::repository::chevereto_import as import_repo;
    import_repo::start(&pool, Some(0), 10, false).await.unwrap();
    import_repo::record_failure(&pool, 3, None, "real")
        .await
        .unwrap();
    import_repo::finish_chunk(&pool, Some(10), 0).await.unwrap();

    import_repo::start(&pool, None, 20, true).await.unwrap();
    import_repo::record_imported(&pool, None).await.unwrap();
    import_repo::record_failure(&pool, 15, None, "dry")
        .await
        .unwrap();
    import_repo::finish_chunk(&pool, None, 0).await.unwrap();
    let row = import_repo::get(&pool).await.unwrap();
    assert_eq!(
        (row.dry_run, row.last_image_id, row.imported, row.failed),
        (true, 10, 1, 2)
    );
}
