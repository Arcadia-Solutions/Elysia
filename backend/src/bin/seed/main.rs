//! Fill the dev database and upload dir with test users and generated images.
//!
//! Every image goes through the real upload pipeline, so sizes, hashes,
//! conversions and thumbnails match what an actual upload stores, then its
//! upload date is moved into the past so the dashboard has history to show.
//!
//! cargo run --manifest-path backend/Cargo.toml --bin seed -- [image count]

use std::io::{Cursor, Write};

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use elysia::config::Config;
use elysia::repository::settings as settings_repo;
use elysia::repository::users;
use elysia::services::auth as auth_service;
use elysia::services::files::{self, UploadOptions};
use elysia::services::image::TargetFormat;
use elysia::storage::Storage;

const USERNAMES: [&str; 12] = [
    "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi", "ivan", "judy", "mallory",
    "oscar",
];
const PASSWORD: &str = "password123";
const DEFAULT_IMAGE_COUNT: usize = 300;
const HISTORY_DAYS: f64 = 3.0 * 365.0;

/// Tiny xorshift generator, seeded per run so reruns add new images instead of
/// deduplicating onto the previous ones.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, low: u32, high: u32) -> u32 {
        low + (self.next() % u64::from(high - low)) as u32
    }
}

/// A two-colour gradient with some noise, so encoded sizes vary like photos do.
fn generate_image(random: &mut Random, width: u32, height: u32) -> RgbImage {
    let start: [u8; 3] = std::array::from_fn(|_| random.next() as u8);
    let end: [u8; 3] = std::array::from_fn(|_| random.next() as u8);
    let noise = random.range(1, 40);
    RgbImage::from_fn(width, height, |x, y| {
        let ratio = (x + y) as f32 / (width + height) as f32;
        let jitter = random.range(0, noise) as u8;
        Rgb(std::array::from_fn(|channel| {
            let value =
                start[channel] as f32 + (end[channel] as f32 - start[channel] as f32) * ratio;
            (value as u8).saturating_add(jitter)
        }))
    })
}

/// Existing seed users are reused, so the binary can run more than once.
async fn seed_users(pool: &PgPool) -> Vec<Uuid> {
    let password_hash = auth_service::hash_password(PASSWORD).expect("failed to hash password");
    let mut user_ids = Vec::new();
    for username in USERNAMES {
        let existing = users::find_by_username_or_email(pool, username)
            .await
            .expect("failed to look up user");
        let user = match existing {
            Some(user) => user,
            None => users::create_local_user(
                pool,
                username,
                Some(&format!("{username}@example.com")),
                &password_hash,
                false,
            )
            .await
            .expect("failed to create user"),
        };
        user_ids.push(user.id);
    }
    user_ids
}

#[tokio::main]
async fn main() {
    let image_count = std::env::args()
        .nth(1)
        .map(|count| count.parse().expect("image count must be a number"))
        .unwrap_or(DEFAULT_IMAGE_COUNT);

    let config = Config::load();
    let pool = PgPoolOptions::new()
        .connect(&config.database.url())
        .await
        .expect("failed to connect to database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");
    let storage = Storage::new(&config.storage.upload_dir);
    storage
        .ensure_root()
        .await
        .expect("failed to create upload dir");
    let settings = settings_repo::load(&pool)
        .await
        .expect("failed to load elysia settings");

    let user_ids = seed_users(&pool).await;
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before 1970")
        .as_nanos() as u64;
    let mut random = Random(seed | 1);

    for index in 0..image_count {
        let (width, height) = (random.range(80, 1200), random.range(80, 1200));
        let pixels = DynamicImage::ImageRgb8(generate_image(&mut random, width, height));

        // Sources are png, jpg or webp (what the image crate encodes); a sixth
        // of them is converted by the pipeline to show more formats.
        let (source_format, source_ext) = match random.range(0, 3) {
            0 => (ImageFormat::Png, "png"),
            1 => (ImageFormat::Jpeg, "jpg"),
            _ => (ImageFormat::WebP, "webp"),
        };
        let mut bytes = Cursor::new(Vec::new());
        pixels
            .write_to(&mut bytes, source_format)
            .expect("failed to encode image");
        let bytes = bytes.into_inner();

        // No jxl: most browsers can't display it, so those images look missing.
        let target_file_format = (random.range(0, 6) == 0).then_some(TargetFormat::Avif);
        let options = UploadOptions {
            lossy_compression_value: target_file_format.map(|_| 75),
            target_file_format,
            strip_exif: false,
        };

        let mut temp = storage.new_temp().expect("failed to create temp file");
        temp.write_all(&bytes).expect("failed to write temp file");

        // Squared draws favour the first users and recent days: a few heavy
        // uploaders and a growing upload rate, like a real instance.
        let uploader = user_ids[(random.unit().powi(2) * user_ids.len() as f64) as usize];
        let stored = match files::store_upload(
            &pool,
            &storage,
            &settings,
            temp,
            bytes.len() as u64,
            Some(format!("seed-{index}.{source_ext}")),
            &options,
            uploader,
            None,
        )
        .await
        {
            Ok(stored) => stored,
            Err(error) => {
                eprintln!("image {index} rejected: {error}");
                continue;
            }
        };
        let seconds_ago = random.unit().powi(2) * HISTORY_DAYS * 86400.0;
        sqlx::query!(
            "update files set created_at = now() - make_interval(secs => $2) where id = $1",
            stored.id,
            seconds_ago,
        )
        .execute(&pool)
        .await
        .expect("failed to backdate file");

        if (index + 1) % 25 == 0 {
            println!("{}/{image_count} images", index + 1);
        }
    }
    // Users were created now; move each sign-up back to their first upload so
    // the new users count matches the upload history.
    sqlx::query!(
        "update users u set created_at = first.created_at \
         from (select uploaded_by_user_id, min(created_at) as created_at \
               from files group by 1) as first \
         where first.uploaded_by_user_id = u.id and u.id = any($1) \
           and first.created_at < u.created_at",
        &user_ids,
    )
    .execute(&pool)
    .await
    .expect("failed to backdate users");
    println!("done, seed users log in with password `{PASSWORD}`");
}
