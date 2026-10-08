//! Helpers shared by the integration test binaries.
#![allow(dead_code)]

use actix_multipart::form::{MultipartFormConfig, tempfile::TempFileConfig};
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::Data;
use actix_web::{App, web};
use sqlx::PgPool;

use elysia::config::{AuthConfig, Config, DatabaseConfig, ServerConfig, StorageConfig};
use elysia::handlers::upload;
use elysia::services::files::generate_id;
use elysia::services::image::{ProcessOptions, TargetFormat};
use elysia::storage::Storage;

pub fn test_config() -> Config {
    Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 8080,
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
            max_file_size_bytes: 0,
            max_width_pixels: 0,
            max_height_pixels: 0,
            target_width_pixels: 0,
            target_height_pixels: 0,
            target_file_format: None,
            target_file_size_bytes: 0,
        },
        auth: AuthConfig {
            admin_token: "admin-token".into(),
        },
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

/// POST `image` to `/api/upload` against an app built from `config` and `pool`,
/// returning the status.
pub async fn upload_status(pool: PgPool, config: Config, image: Vec<u8>) -> StatusCode {
    upload_status_with_query(pool, config, image, "").await
}

/// Like `upload_status`, with a query string appended to the request uri.
pub async fn upload_status_with_query(
    pool: PgPool,
    config: Config,
    image: Vec<u8>,
    query: &str,
) -> StatusCode {
    let upload_dir = std::env::temp_dir().join(format!("elysia-test-{}", generate_id()));
    std::fs::create_dir_all(&upload_dir).unwrap();
    let token = config.auth.admin_token.clone();
    let storage = Storage::new(upload_dir.to_str().unwrap());

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(config))
            .app_data(Data::new(pool))
            .app_data(Data::new(storage))
            .app_data(MultipartFormConfig::default().total_limit(10 * 1024 * 1024))
            .app_data(TempFileConfig::default().directory(&upload_dir))
            .route("/api/upload", web::post().to(upload)),
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

    let request = actix_test::TestRequest::post()
        .uri(&format!("/api/upload?{query}"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(body)
        .to_request();

    let status = actix_test::call_service(&app, request).await.status();
    let _ = std::fs::remove_dir_all(&upload_dir);
    status
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

/// Build a Storage over a fresh temp dir for rehost tests.
pub fn temp_storage() -> (Storage, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("elysia-test-{}", generate_id()));
    std::fs::create_dir_all(&dir).unwrap();
    (Storage::new(dir.to_str().unwrap()), dir)
}

pub fn webp_config() -> Config {
    let mut c = test_config();
    c.storage.target_file_format = Some(TargetFormat::Webp);
    c
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
