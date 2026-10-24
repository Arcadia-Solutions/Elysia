//! Helpers shared by the integration test binaries.
#![allow(dead_code)]

use std::sync::RwLock;

use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use sqlx::PgPool;
use uuid::Uuid;

use elysia::config::{
    AuthConfig, Config, DatabaseConfig, ServerConfig, SsoConfig, StorageConfig, TelemetryConfig,
};
use elysia::repository::files::{self, NewFile};
use elysia::repository::users::{self, AuthUserRow};
use elysia::routes;
use elysia::services::auth as auth_service;
use elysia::services::files::generate_id;
use elysia::services::image::{ProcessOptions, TargetFormat};
use elysia::settings::ElysiaSettings;
use elysia::storage::Storage;

pub fn test_config() -> Config {
    Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 8080,
            public_url: "http://elysia.test".into(),
        },
        database: DatabaseConfig {
            host: "localhost".into(),
            port: 5432,
            user: "elysia".into(),
            password: "secret".into(),
            name: "elysia".into(),
        },
        storage: StorageConfig {
            upload_dir: "/tmp/elysia".into(),
        },
        auth: AuthConfig {
            jwt_secret: "test-jwt-secret-with-enough-bytes".into(),
            sso: None,
        },
        telemetry: TelemetryConfig::default(),
        rate_limit: None,
        outbound_proxy: None,
    }
}

/// Test config with single sign-on enabled against a mock issuer. The
/// redirect URI is never visited in tests; it is only echoed into the
/// authorize URL and the token request.
pub fn sso_config(issuer: &str) -> Config {
    let mut config = test_config();
    config.auth.sso = Some(SsoConfig {
        name: "Arcadia".into(),
        issuer: issuer.into(),
        client_id: "elysia-test-client".into(),
        client_secret: "elysia-test-secret".into(),
        redirect_uri: "http://localhost:3000/auth/sso/callback".into(),
        status_endpoint: Some(format!("{issuer}/users/{{sub}}/status")),
    });
    config
}

/// Default elysia settings: all checks off and processing off. Note this fails
/// `validate` (max_file_size_bytes is 0) by design, which the validation tests
/// rely on; the store/upload paths never call `validate`, so 0 = unlimited there.
pub fn test_settings() -> ElysiaSettings {
    ElysiaSettings {
        max_file_size_bytes: 0,
        max_width_pixels: 0,
        max_height_pixels: 0,
        target_width_pixels: 0,
        target_height_pixels: 0,
        default_target_file_format: None,
        allow_overriding_file_format: false,
        target_file_size_bytes: 0,
        default_compression: 0,
        allow_overriding_compression: true,
        strip_exif_by_default: false,
        allow_overriding_strip_exif: true,
        thumbnail_width_pixels: 0,
        thumbnail_height_pixels: 0,
        thumbnail_quality: 80,
        allow_registration: true,
        allowed_referrers: Vec::new(),
        uploads_enabled: true,
        default_storage_quota_bytes: 0,
        record_referrer_views: false,
    }
}

/// Default settings with WebP processing turned on.
pub fn webp_settings() -> ElysiaSettings {
    ElysiaSettings {
        default_target_file_format: Some(TargetFormat::Webp),
        ..test_settings()
    }
}

/// PNG/zlib CRC-32 (IEEE), computed over the chunk type + data.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Smallest thing `infer` reads as a PNG and `imagesize` reads dimensions
/// from: the 8-byte signature plus a valid IHDR chunk, no image data.
pub fn tiny_png(width: u32, height: u32) -> Vec<u8> {
    let mut ihdr = b"IHDR".to_vec();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // bit depth, color type, compression, filter, interlace

    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13u32.to_be_bytes()); // IHDR data length
    bytes.extend_from_slice(&ihdr);
    bytes.extend_from_slice(&crc32(&ihdr).to_be_bytes());
    bytes
}

/// Create a local user in `pool` and mint a session token for them. Used by
/// tests that need an authenticated (non-admin) caller.
pub async fn test_user(pool: &PgPool, username: &str) -> (AuthUserRow, String) {
    let password_hash = auth_service::hash_password("long-test-password").unwrap();
    let email = format!("{username}@example.com");
    let row = users::create_local_user(pool, username, Some(&email), &password_hash, false)
        .await
        .unwrap();
    let config = test_config();
    let token = auth_service::mint_jwt(&row.id, &config.auth.jwt_secret).unwrap();
    (row, token)
}

/// POST `image` to `/api/upload` against an app built from `settings` and `pool`,
/// returning the status.
pub async fn upload_status(pool: PgPool, settings: ElysiaSettings, image: Vec<u8>) -> StatusCode {
    upload_status_with_query(pool, settings, image, "").await
}

/// Like `upload_status`, with a query string appended to the request uri.
pub async fn upload_status_with_query(
    pool: PgPool,
    settings: ElysiaSettings,
    image: Vec<u8>,
    query: &str,
) -> StatusCode {
    let (_, token) = test_user(&pool, &format!("uploader-{}", generate_id())).await;
    upload_with_token(pool, settings, image, query, &token)
        .await
        .0
}

/// POST `image` to `/api/upload` with `token` as the bearer, returning the
/// status and the parsed JSON body.
pub async fn upload_with_token(
    pool: PgPool,
    settings: ElysiaSettings,
    image: Vec<u8>,
    query: &str,
    token: &str,
) -> (StatusCode, serde_json::Value) {
    upload_with_headers(
        pool,
        test_config(),
        settings,
        image,
        query,
        &[("Authorization", format!("Bearer {token}"))],
    )
    .await
}

/// POST `image` to `/api/upload` against an app built from `config`, with
/// the given extra request headers, returning the status and the parsed JSON
/// body.
pub async fn upload_with_headers(
    pool: PgPool,
    config: Config,
    settings: ElysiaSettings,
    image: Vec<u8>,
    query: &str,
    headers: &[(&str, String)],
) -> (StatusCode, serde_json::Value) {
    let upload_dir = tempfile::tempdir().unwrap();
    let storage = Storage::new(upload_dir.path().to_str().unwrap());

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(config))
            .app_data(Data::new(pool))
            .app_data(Data::new(storage))
            .app_data(Data::new(RwLock::new(settings)))
            .configure(routes::init),
    )
    .await;

    let boundary = "testboundary";
    let mut body = format!("--{boundary}\r\n").into_bytes();
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"x.png\"\r\n\
          Content-Type: image/png\r\n\r\n",
    );
    body.extend_from_slice(&image);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    // Extra headers go last, so they may override the Content-Length set_payload sets.
    let mut request = actix_test::TestRequest::post()
        .uri(&format!("/api/upload?{query}"))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(body);
    for (name, value) in headers {
        request = request.insert_header((*name, value.as_str()));
    }
    let request = request.to_request();

    let response = actix_test::call_service(&app, request).await;
    let status = response.status();
    let response_body: serde_json::Value = actix_test::read_body_json(response).await;
    (status, response_body)
}

/// One-shot HTTP server on a loopback port: answers a single request with the
/// given status line, content type and body, then closes. Returns its URL.
pub fn serve_once(status_line: &'static str, content_type: &'static str, body: Vec<u8>) -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.read(&mut [0u8; 1024]); // drain the request line/headers
            let header = format!(
                "{status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    format!("http://{addr}/image.png")
}

/// Build a Storage over a fresh temp dir, deleted when the returned `TempDir` drops.
pub fn temp_storage() -> (Storage, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("trash")).unwrap();
    (Storage::new(dir.path().to_str().unwrap()), dir)
}

pub fn real_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([120, 30, 200, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

/// A noisy image: a flat colour compresses to almost nothing losslessly.
pub fn noisy_png(width: u32, height: u32) -> Vec<u8> {
    let noisy = image::RgbaImage::from_fn(width, height, |x, y| {
        let v = (x * 7919 + y * 104729 + x * y * 31) as u8;
        image::Rgba([v, v.wrapping_mul(3), v.wrapping_add(90), 255])
    });
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(noisy)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap();
    cursor.into_inner()
}

pub fn animated_gif() -> Vec<u8> {
    use image::{Frame, RgbaImage, codecs::gif::GifEncoder};
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut encoder = GifEncoder::new(&mut buf);
        for color in [[200, 0, 0, 255], [0, 0, 200, 255]] {
            encoder
                .encode_frame(Frame::new(RgbaImage::from_pixel(4, 4, image::Rgba(color))))
                .unwrap();
        }
    }
    buf.into_inner()
}

pub fn pipeline_options(format: TargetFormat) -> ProcessOptions {
    ProcessOptions {
        target_width: 0,
        target_height: 0,
        format,
        target_file_size: 0,
        requested_quality: None,
    }
}

/// Insert `count` bare PNG rows of `size` bytes uploaded by `uploader`, with
/// ids and hashes unique per `prefix`.
pub async fn seed_files(pool: &PgPool, uploader: Uuid, prefix: &str, count: usize, size: i64) {
    for index in 0..count {
        let id = format!("{prefix}{index:06}");
        files::insert(
            pool,
            &NewFile {
                id: &id,
                ext: "png",
                mime: "image/png",
                original_name: None,
                size,
                width: 1,
                height: 1,
                hash: &id,
                original_hash: &id,
                original_ext: "png",
                original_width: 1,
                original_height: 1,
                requested_quality: None,
                applied_quality: None,
                has_thumbnail: false,
                uploaded_by_user_id: uploader,
                api_key_id: None,
            },
        )
        .await
        .unwrap();
    }
}

/// Newest available images first, on the given browse page.
pub fn page(page: u32) -> elysia::repository::files::ImagesQuery {
    use elysia::repository::files::{ImagesQuery, ImagesSortBy, ImagesStatus, SortOrder};
    ImagesQuery {
        page: std::num::NonZeroU32::new(page).unwrap(),
        page_size: 100,
        sort_by: ImagesSortBy::CreatedAt,
        order: SortOrder::Desc,
        status: ImagesStatus::Available,
        username: None,
        album_id: None,
    }
}
