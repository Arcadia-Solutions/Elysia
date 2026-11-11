//! Fill the database and upload dir with test users and generated images.
//!
//! Every image goes through the real upload pipeline, so sizes, hashes,
//! conversions and medium images match what an actual upload stores, then its
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
use crate::repository::{files as files_repository, users};
use crate::services::auth as auth_service;
use crate::services::files::{self, UploadOptions};
use crate::services::image::TargetFormat;
use crate::services::{check_shutdown, track_job};
use crate::settings::ElysiaSettings;
use crate::storage::Storage;

const USERNAMES: [&str; 12] = [
    "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi", "ivan", "judy", "mallory",
    "oscar",
];
/// `Referer` hosts the fixture views come from, most popular first; empty is
/// no `Referer`. More than the dashboard lists on their own, so it shows others.
const REFERRERS: [&str; 12] = [
    "",
    "forum.example.com",
    "reddit.com",
    "discord.com",
    "blog.example.org",
    "news.ycombinator.com",
    "mastodon.social",
    "github.com",
    "lemmy.world",
    "wiki.example.net",
    "shop.example.com",
    "mirror.example.org",
];
const MAX_IMAGE_COUNT: u32 = 10_000;
const MAX_HISTORY_DAYS: u32 = 10 * 365;
/// Generation floods the instance with fake users and images, so only debug
/// builds (`cargo run`, tests, e2e) allow it, never the release image.
pub const AVAILABLE: bool = cfg!(debug_assertions);

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
    /// Whether this build allows generation, false on release builds.
    pub available: bool,
    pub running: bool,
    /// Images handled so far, rejected ones included.
    pub done: u32,
    pub total: u32,
    /// Why the last run stopped early, if it did.
    pub error: Option<String>,
}

// ponytail: one run at a time per process, fine for a dev tool.
static PROGRESS: Mutex<FixturesProgress> = Mutex::new(FixturesProgress {
    available: AVAILABLE,
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

/// Start a run in the background. Fails on release builds or if one is already running.
pub fn start(
    pool: PgPool,
    storage: Data<Storage>,
    settings: ElysiaSettings,
    params: FixturesParams,
) -> Result<()> {
    if !AVAILABLE {
        return Err(Error::Forbidden);
    }
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
            available: AVAILABLE,
            running: true,
            done: 0,
            total: params.image_count,
            error: None,
        };
    }
    tokio::spawn(track_job(async move {
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
    }));
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

/// Landscape palettes: sky top, horizon, nearest mountains, sun.
const PALETTES: [[[f32; 3]; 4]; 6] = [
    // dusk
    [
        [25., 35., 90.],
        [240., 150., 110.],
        [30., 20., 50.],
        [255., 240., 180.],
    ],
    // dawn
    [
        [120., 160., 220.],
        [255., 200., 170.],
        [60., 50., 90.],
        [255., 250., 220.],
    ],
    // day
    [
        [60., 130., 220.],
        [190., 225., 250.],
        [30., 70., 60.],
        [255., 255., 235.],
    ],
    // night
    [
        [5., 10., 30.],
        [40., 60., 110.],
        [5., 8., 20.],
        [230., 235., 255.],
    ],
    // forest haze
    [
        [150., 190., 170.],
        [230., 240., 210.],
        [20., 50., 35.],
        [255., 250., 220.],
    ],
    // desert
    [
        [200., 110., 60.],
        [255., 215., 150.],
        [90., 40., 30.],
        [255., 245., 200.],
    ],
];

fn mix(from: [f32; 3], to: [f32; 3], ratio: f32) -> [f32; 3] {
    std::array::from_fn(|channel| from[channel] + (to[channel] - from[channel]) * ratio)
}

/// One mountain ridgeline: per column height in [0, 1], fractal value noise.
fn ridge(random: &mut Random, width: u32) -> Vec<f32> {
    let octaves: Vec<(f32, Vec<f32>)> = (0..7)
        .map(|octave| {
            let cells = 2u32 << octave;
            let lattice = (0..=cells + 1).map(|_| random.unit() as f32).collect();
            (cells as f32, lattice)
        })
        .collect();
    (0..width)
        .map(|x| {
            let position = x as f32 / width as f32;
            let (mut sum, mut amplitude) = (0.0, 0.5);
            for (cells, lattice) in &octaves {
                let at = position * cells;
                let (cell, fraction) = (at as usize, at.fract());
                let smooth = fraction * fraction * (3.0 - 2.0 * fraction);
                sum += amplitude * (lattice[cell] + (lattice[cell + 1] - lattice[cell]) * smooth);
                amplitude *= 0.55;
            }
            sum
        })
        .collect()
}

/// Layered mountains under a sky with a sun, plus grain so encoded sizes vary
/// like photos do.
fn generate_image(random: &mut Random, width: u32, height: u32) -> RgbImage {
    let [sky_top, horizon, near, sun] = PALETTES[random.range(0, PALETTES.len() as u32) as usize];
    let (width_f32, height_f32) = (width as f32, height as f32);
    let (sun_x, sun_y) = (
        width_f32 * random.unit() as f32,
        height_f32 * (0.2 + 0.3 * random.unit() as f32),
    );
    let sun_radius = width_f32.min(height_f32) * (0.03 + 0.05 * random.unit() as f32);
    // Far to near: each layer sits lower, and far ones fade into the horizon haze.
    let layer_count = random.range(3, 7);
    let layers: Vec<(Vec<f32>, f32, f32, [f32; 3])> = (0..layer_count)
        .map(|layer| {
            let depth = (layer + 1) as f32 / layer_count as f32;
            let base = height_f32 * (0.35 + 0.45 * depth);
            let amplitude = height_f32 * (0.35 - 0.15 * depth);
            (
                ridge(random, width),
                base,
                amplitude,
                mix(horizon, near, 0.25 + 0.75 * depth),
            )
        })
        .collect();
    let grain = random.range(1, 16);
    RgbImage::from_fn(width, height, |x, y| {
        let (pixel_x, pixel_y) = (x as f32, y as f32);
        let terrain = layers
            .iter()
            .rev()
            .find(|(ridge, base, amplitude, _)| pixel_y >= base - ridge[x as usize] * amplitude);
        let color = match terrain {
            Some((_, _, _, color)) => *color,
            None => {
                let distance = ((pixel_x - sun_x).powi(2) + (pixel_y - sun_y).powi(2)).sqrt();
                if distance < sun_radius {
                    sun
                } else {
                    let sky = mix(sky_top, horizon, (pixel_y / (height_f32 * 0.75)).min(1.0));
                    let glow = (-(distance - sun_radius) / (sun_radius * 4.0)).exp() * 0.6;
                    mix(sky, sun, glow)
                }
            }
        };
        let jitter = random.range(0, grain) as f32;
        Rgb(color.map(|value| (value + jitter).min(255.0) as u8))
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
    // Views of every stored image, one entry per (id, referrer), added at the end.
    let (mut view_ids, mut view_referrers, mut views, mut medium_views, mut thumbnail_views) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

    for index in 0..params.image_count {
        check_shutdown()?;
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
            expiry_days: None,
            remaining_storage_bytes: None,
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
                    r#"update files set created_at = now() - make_interval(secs => $2) where id = $1"#,
                    stored.id,
                    seconds_ago,
                )
                .execute(pool)
                .await?;
                // Up to 3 distinct consecutive referrers from a squared draw, so
                // the first ones get most of the traffic, with heavy-tailed counts.
                let first = (random.unit().powi(2) * REFERRERS.len() as f64) as usize;
                for offset in 0..random.range(0, 4) as usize {
                    view_ids.push(stored.id.clone());
                    view_referrers.push(REFERRERS[(first + offset) % REFERRERS.len()].to_owned());
                    views.push((random.unit().powi(3) * 5000.0) as i64);
                    medium_views.push((random.unit().powi(3) * 1000.0) as i64);
                    thumbnail_views.push((random.unit().powi(3) * 1000.0) as i64);
                }
            }
            Err(error) => tracing::warn!("fixture image {index} rejected: {error}"),
        }
        update(|progress| progress.done = index + 1);
    }
    files_repository::add_views(
        pool,
        &view_ids,
        &view_referrers,
        &views,
        &medium_views,
        &thumbnail_views,
    )
    .await?;
    // Users were created now; move each sign-up back to their first upload so
    // the new users count matches the upload history.
    sqlx::query!(
        r#"update users u set created_at = first.created_at
         from (select uploaded_by_user_id, min(created_at) as created_at
               from files group by 1) as first
         where first.uploaded_by_user_id = u.id and u.id = any($1)
           and first.created_at < u.created_at"#,
        &user_ids,
    )
    .execute(pool)
    .await?;
    Ok(())
}
