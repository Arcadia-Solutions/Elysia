//! Fill the database and upload dir with test users and generated images.
//!
//! Every image goes through the real upload pipeline, so sizes, hashes,
//! conversions and thumbnails match what an actual upload stores, then its
//! upload date is moved into the past so the dashboard has history to show.

use std::io::{Cursor, Write};
use std::sync::Mutex;

use actix_web::web::Data;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::repository::users;
use crate::services::auth as auth_service;
use crate::services::files::{self, UploadOptions};
use crate::services::image::TargetFormat;
use crate::settings::ElysiaSettings;
use crate::storage::Storage;

const USERNAMES: [&str; 12] = [
    "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi", "ivan", "judy", "mallory",
    "oscar",
];
const MAX_IMAGE_COUNT: u32 = 10_000;
const MAX_HISTORY_DAYS: u32 = 10 * 365;

#[derive(Deserialize, ToSchema)]
pub struct FixturesParams {
    /// Images to generate, at most 10000.
    pub image_count: u32,
    /// Fixture users the images are spread over (`alice`, `bob`, ..., then `alice2`, ...).
    pub user_count: u32,
    /// Upload dates are spread over this many past days, at most 3650.
    pub history_days: u32,
    /// Password given to the fixture users this run creates. Existing ones keep theirs.
    pub password: String,
}

/// State of the current or last generation run.
#[derive(Clone, Default, Serialize, ToSchema)]
pub struct FixturesProgress {
    pub running: bool,
    /// Images handled so far, rejected ones included.
    pub done: u32,
    pub total: u32,
    /// Why the last run stopped early, if it did.
    pub error: Option<String>,
}

// ponytail: one run at a time per process, fine for a dev tool.
static PROGRESS: Mutex<FixturesProgress> = Mutex::new(FixturesProgress {
    running: false,
    done: 0,
    total: 0,
    error: None,
});

fn update(change: impl FnOnce(&mut FixturesProgress)) {
    change(&mut PROGRESS.lock().expect("fixtures lock poisoned"));
}

pub fn progress() -> FixturesProgress {
    PROGRESS.lock().expect("fixtures lock poisoned").clone()
}

/// Start a run in the background. Fails if one is already running.
pub fn start(
    pool: PgPool,
    storage: Data<Storage>,
    settings: ElysiaSettings,
    params: FixturesParams,
) -> Result<()> {
    if params.image_count == 0 || params.image_count > MAX_IMAGE_COUNT {
        return Err(Error::BadRequest(format!(
            "image count must be between 1 and {MAX_IMAGE_COUNT}"
        )));
    }
    if params.user_count == 0 {
        return Err(Error::BadRequest("user count must be at least 1".into()));
    }
    if params.history_days == 0 || params.history_days > MAX_HISTORY_DAYS {
        return Err(Error::BadRequest(format!(
            "history days must be between 1 and {MAX_HISTORY_DAYS}"
        )));
    }
    if params.password.is_empty() {
        return Err(Error::BadRequest("password must not be empty".into()));
    }
    {
        let mut progress = PROGRESS.lock().expect("fixtures lock poisoned");
        if progress.running {
            return Err(Error::Conflict);
        }
        *progress = FixturesProgress {
            running: true,
            done: 0,
            total: params.image_count,
            error: None,
        };
    }
    tokio::spawn(async move {
        // Run in its own task so a panic still clears `running` below.
        let result = tokio::spawn(async move { run(&pool, &storage, &settings, &params).await })
            .await
            .unwrap_or(Err(Error::Internal));
        if let Err(error) = &result {
            tracing::error!("fixtures generation failed: {error}");
        }
        update(|progress| {
            progress.running = false;
            progress.error = result.err().map(|error| error.to_string());
        });
    });
    Ok(())
}

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

/// Generate one random png, jpg or webp (what the image crate encodes),
/// returning its bytes and extension.
fn encode_random_image(seed: u64) -> Result<(Vec<u8>, &'static str)> {
    let mut random = Random(seed | 1);
    let (width, height) = (random.range(80, 1200), random.range(80, 1200));
    let pixels = DynamicImage::ImageRgb8(generate_image(&mut random, width, height));
    let (format, extension) = match random.range(0, 3) {
        0 => (ImageFormat::Png, "png"),
        1 => (ImageFormat::Jpeg, "jpg"),
        _ => (ImageFormat::WebP, "webp"),
    };
    let mut bytes = Cursor::new(Vec::new());
    pixels.write_to(&mut bytes, format).map_err(|error| {
        tracing::error!("failed to encode fixture image: {error}");
        Error::Internal
    })?;
    Ok((bytes.into_inner(), extension))
}

/// Existing fixture users are reused, so generation can run more than once.
async fn fixture_users(pool: &PgPool, count: usize, password: &str) -> Result<Vec<Uuid>> {
    let password_hash = auth_service::hash_password_async(password.to_owned()).await?;
    let mut user_ids = Vec::new();
    for index in 0..count {
        // Past the name list, cycle it with a round suffix: `alice2`, `bob2`, ...
        let name = USERNAMES[index % USERNAMES.len()];
        let username = match index / USERNAMES.len() {
            0 => name.to_owned(),
            round => format!("{name}{}", round + 1),
        };
        let user = match users::find_by_username_or_email(pool, &username).await? {
            Some(user) => user,
            None => {
                users::create_local_user(
                    pool,
                    &username,
                    Some(&format!("{username}@example.com")),
                    &password_hash,
                    false,
                )
                .await?
            }
        };
        user_ids.push(user.id);
    }
    Ok(user_ids)
}

async fn run(
    pool: &PgPool,
    storage: &Storage,
    settings: &ElysiaSettings,
    params: &FixturesParams,
) -> Result<()> {
    let user_ids = fixture_users(pool, params.user_count as usize, &params.password).await?;
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before 1970")
        .as_nanos() as u64;
    let mut random = Random(seed | 1);
    let history_seconds = f64::from(params.history_days) * 86400.0;

    for index in 0..params.image_count {
        // Encoding is CPU bound, keep it off the server's async workers.
        let image_seed = random.next();
        let (bytes, source_ext) =
            tokio::task::spawn_blocking(move || encode_random_image(image_seed))
                .await
                .map_err(|_| Error::Internal)??;

        // A sixth of the images is converted by the pipeline to show more
        // formats. No jxl: most browsers can't display it, so those images
        // look missing.
        let target_file_format = (random.range(0, 6) == 0).then_some(TargetFormat::Avif);
        let options = UploadOptions {
            lossy_compression_value: target_file_format.map(|_| 75),
            target_file_format,
            strip_exif: false,
        };

        let mut temp = storage.new_temp()?;
        temp.write_all(&bytes)?;

        // Squared draws favour the first users and recent days: a few heavy
        // uploaders and a growing upload rate, like a real instance.
        let uploader = user_ids[(random.unit().powi(2) * user_ids.len() as f64) as usize];
        match files::store_upload(
            pool,
            storage,
            settings,
            temp,
            bytes.len() as u64,
            Some(format!("fixture-{index}.{source_ext}")),
            &options,
            uploader,
            None,
        )
        .await
        {
            Ok(stored) => {
                let seconds_ago = random.unit().powi(2) * history_seconds;
                sqlx::query!(
                    "update files set created_at = now() - make_interval(secs => $2) where id = $1",
                    stored.id,
                    seconds_ago,
                )
                .execute(pool)
                .await?;
            }
            Err(error) => tracing::warn!("fixture image {index} rejected: {error}"),
        }
        update(|progress| progress.done = index + 1);
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
    .execute(pool)
    .await?;
    Ok(())
}
