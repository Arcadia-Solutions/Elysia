//! Copy every image of a Chevereto v4 instance into Elysia and rewrite the
//! Chevereto links stored in a backend's database.
//!
//! Images go through the real upload pipeline with the size limits lifted,
//! keep their uploader and upload date, and are processed in chunks whose
//! progress is saved to a state file, so the import can be stopped and rerun.
//!
//! cargo run --release --manifest-path backend/Cargo.toml --bin import_chevereto -- import.yml

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection, PgPool};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use uuid::Uuid;

use elysia::chevereto_import::{self, BackendUsers, CheveretoImage, LinkSettings};
use elysia::config::Config;
use elysia::handlers::images::upload::UploadOptionsQuery;
use elysia::repository::files as files_repo;
use elysia::repository::settings as settings_repo;
use elysia::repository::users;
use elysia::services::auth as auth_service;
use elysia::services::files;
use elysia::settings::ElysiaSettings;
use elysia::storage::Storage;

/// The import YAML, see `import.example.yml`.
#[derive(Deserialize)]
struct ImportConfig {
    chevereto: CheveretoConfig,
    backend: Option<BackendConfig>,
    elysia: ElysiaConfig,
    fallback_username: String,
    batch_size: u64,
    concurrency: usize,
    state_file: String,
    failed_file: String,
}

#[derive(Deserialize)]
struct CheveretoConfig {
    database_url: String,
    public_url: String,
    #[serde(default)]
    link_hosts: Vec<String>,
}

#[derive(Deserialize)]
struct BackendConfig {
    database_url: String,
    users: Option<BackendUsers>,
}

#[derive(Deserialize)]
struct ElysiaConfig {
    public_url: String,
}

/// Shared by every upload task.
struct Context {
    pool: PgPool,
    storage: Storage,
    settings: ElysiaSettings,
    link_settings: LinkSettings,
    http_client: reqwest::Client,
}

/// An image that could not be imported, for the failed file.
struct Failure {
    image_id: u64,
    fetch_url: Option<String>,
    error: String,
}

/// One chunk of images after the saved id, with storage and uploader.
const CHUNK_QUERY: &str = "select cast(i.image_id as unsigned), i.image_name, i.image_extension, \
     cast(i.image_storage_mode as char), date_format(i.image_date, '%Y/%m/%d'), \
     cast(nullif(unix_timestamp(i.image_date_gmt), 0) as signed), i.image_original_filename, \
     s.storage_url, u.user_username \
     from chv_images i \
     left join chv_storages s on s.storage_id = i.image_storage_id \
     left join chv_users u on u.user_id = i.image_user_id \
     where i.image_type = 1 and i.image_id > ? \
     order by i.image_id limit ?";

/// A `CHUNK_QUERY` row, in select order.
type ChunkRow = (
    u64,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Panics unless Chevereto uses the default paths the link patterns assume.
async fn check_chevereto_settings(chevereto: &MySqlPool) {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "select setting_name, setting_value from chv_settings \
         where setting_name in ('upload_image_path', 'route_image', 'root_route')",
    )
    .fetch_all(chevereto)
    .await
    .expect("failed to read chv_settings");
    let setting = |name: &str| {
        rows.iter()
            .find(|(setting_name, _)| setting_name == name)
            .and_then(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };
    if let Some(value) = setting("upload_image_path") {
        assert!(
            value == "images",
            "chevereto upload_image_path is '{value}', only the default 'images' is supported"
        );
    }
    if let Some(value) = setting("route_image") {
        assert!(
            value == "image",
            "chevereto route_image is '{value}', only the default 'image' is supported"
        );
    }
    assert!(
        setting("root_route").as_deref() != Some("image"),
        "chevereto root_route is 'image', which makes image links ambiguous and is not supported"
    );
}

/// A trimmed `chv_variables` value, `None` when the variable is missing or empty.
async fn chevereto_variable(chevereto: &MySqlPool, name: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "select variable_value from chv_variables where variable_name = ?",
    )
    .bind(name)
    .fetch_optional(chevereto)
    .await
    .expect("failed to read chv_variables")
    .flatten()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

/// How uploaders are matched to backend users.
struct UserLookup {
    issuer: String,
    users: BackendUsers,
}

/// Elysia user per Chevereto username, resolved on first use: the SSO account
/// of the backend user with the same name, or the fallback user when there is
/// none, no user lookup is configured, or for guest uploads.
struct Uploaders {
    lookup: Option<UserLookup>,
    fallback_username: String,
    fallback_user_id: Option<Uuid>,
    by_username: HashMap<String, Uuid>,
}

impl Uploaders {
    async fn resolve(
        &mut self,
        pool: &PgPool,
        backend: Option<&mut PgConnection>,
        username: Option<&str>,
    ) -> Uuid {
        let (Some(username), Some(backend)) = (username, backend) else {
            return self.fallback(pool).await;
        };
        let Some(lookup) = &self.lookup else {
            return self.fallback(pool).await;
        };
        if let Some(user_id) = self.by_username.get(username) {
            return *user_id;
        }
        let backend_user = chevereto_import::find_backend_user(backend, &lookup.users, username)
            .await
            .expect("failed to look up backend user");
        let user_id = match backend_user {
            Some((subject, email)) => {
                users::upsert_sso_user(pool, &lookup.issuer, &subject, username, email.as_deref())
                    .await
                    .expect("failed to provision sso user")
                    .id
            }
            None => {
                println!(
                    "chevereto user '{username}' has no backend account, \
                     their images go to '{}'",
                    self.fallback_username
                );
                self.fallback(pool).await
            }
        };
        self.by_username.insert(username.to_string(), user_id);
        user_id
    }

    /// The fallback user, looked up or created on first use.
    async fn fallback(&mut self, pool: &PgPool) -> Uuid {
        if let Some(user_id) = self.fallback_user_id {
            return user_id;
        }
        let user = match users::find_by_username_or_email(pool, &self.fallback_username)
            .await
            .expect("failed to look up fallback user")
        {
            Some(user) => user,
            None => {
                let password_hash = auth_service::hash_password(&nanoid::nanoid!(32))
                    .expect("failed to hash password");
                users::create_local_user(pool, &self.fallback_username, None, &password_hash, false)
                    .await
                    .expect("failed to create fallback user")
            }
        };
        self.fallback_user_id = Some(user.id);
        user.id
    }
}

/// Downloads one image, stores it and returns its link rewrites.
async fn import_image(
    context: &Context,
    image: &CheveretoImage,
    uploader: Uuid,
) -> Result<Vec<(String, String)>, Failure> {
    let fetch_url = chevereto_import::fetch_url(image, &context.link_settings);
    let failure = |error: String| Failure {
        image_id: image.id,
        fetch_url: fetch_url.clone(),
        error,
    };
    let Some(url) = &fetch_url else {
        return Err(failure("unsupported storage mode".to_string()));
    };
    println!("image {}: importing {url}", image.id);
    // Same defaults as an upload with no per-upload overrides.
    let options = UploadOptionsQuery::default()
        .into_options(&context.settings)
        .map_err(|error| failure(error.to_string()))?;
    let stored = files::store_from_url(
        &context.pool,
        &context.storage,
        &context.settings,
        &context.http_client,
        url,
        image.original_filename.clone(),
        &options,
        uploader,
        None,
    )
    .await
    .map_err(|error| failure(error.to_string()))?;
    if let Some(uploaded_at) = image.uploaded_at {
        files_repo::backdate_created_at(&context.pool, &stored.id, uploaded_at)
            .await
            .map_err(|error| failure(error.to_string()))?;
    }
    println!("image {}: imported as {}", image.id, stored.id);
    Ok(chevereto_import::link_rewrites(
        image,
        &context.link_settings,
        &stored.id,
        &stored.ext,
        stored.has_thumbnail,
    )
    .unwrap_or_default())
}

/// Last fully processed Chevereto image id, 0 before the first chunk.
fn read_state(path: &str) -> u64 {
    match std::fs::read_to_string(path) {
        Ok(content) => content.trim().parse().expect("state file is not a number"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => panic!("failed to read state file: {error}"),
    }
}

/// Writes through a temp file and a rename, so a crash never leaves it half written.
fn write_state(path: &str, last_image_id: u64) {
    let temp_path = format!("{path}.tmp");
    std::fs::write(&temp_path, last_image_id.to_string()).expect("failed to write state file");
    std::fs::rename(&temp_path, path).expect("failed to replace state file");
}

/// Drops failed file lines after `last_image_id`: those images are replayed
/// and logged again if they still fail.
fn prune_failures(path: &str, last_image_id: u64) {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to read failed file: {error}"),
    };
    let kept: String = content
        .lines()
        .filter(|line| {
            line.split(',')
                .next()
                .and_then(|image_id| image_id.parse::<u64>().ok())
                .is_some_and(|image_id| image_id <= last_image_id)
        })
        .map(|line| format!("{line}\n"))
        .collect();
    std::fs::write(path, kept).expect("failed to write failed file");
}

/// Appends one CSV line (`image_id,"fetch_url","error"`) to the failed file.
fn append_failure(path: &str, failure: &Failure) {
    let mut failed_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("failed to open failed file");
    writeln!(
        failed_file,
        "{},\"{}\",\"{}\"",
        failure.image_id,
        failure
            .fetch_url
            .as_deref()
            .unwrap_or_default()
            .replace('"', "\"\""),
        failure.error.replace('"', "\"\"").replace('\n', " ")
    )
    .expect("failed to write failed file");
}

#[tokio::main]
async fn main() {
    let import_path = std::env::args()
        .nth(1)
        .expect("usage: import_chevereto <import.yml>");
    let mut import: ImportConfig = config::Config::builder()
        .add_source(config::File::with_name(&import_path))
        .build()
        .expect("failed to read import config")
        .try_deserialize()
        .expect("invalid import config");
    assert!(import.concurrency > 0, "concurrency must be greater than 0");
    assert!(import.batch_size > 0, "batch_size must be greater than 0");
    if import.chevereto.link_hosts.is_empty() {
        let public_url =
            url::Url::parse(&import.chevereto.public_url).expect("invalid chevereto.public_url");
        assert!(public_url.has_host(), "chevereto.public_url has no host");
        // Keeps a non default port, links stored in the backend include it.
        let host = &public_url[url::Position::BeforeHost..url::Position::AfterPort];
        import.chevereto.link_hosts = vec![host.to_string()];
    }

    let config = Config::load();
    let lookup = import
        .backend
        .as_mut()
        .and_then(|backend| backend.users.take())
        .map(|users| UserLookup {
            issuer: config
                .auth
                .sso
                .as_ref()
                .expect("auth.sso must be configured in config.yml when backend.users is set: matched uploaders become SSO users")
                .normalized_issuer()
                .to_string(),
            users,
        });
    let pool = PgPoolOptions::new()
        .connect(&config.database.url())
        .await
        .expect("failed to connect to elysia database");
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
    let settings = ElysiaSettings {
        max_file_size_bytes: 0,
        max_width_pixels: 0,
        max_height_pixels: 0,
        ..settings
    };

    let chevereto = MySqlPoolOptions::new()
        .max_connections(2)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("set time_zone = '+00:00'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&import.chevereto.database_url)
        .await
        .expect("failed to connect to chevereto database");
    check_chevereto_settings(&chevereto).await;
    // A wrong salt maps viewer links to other images, so never guess it. It is
    // only needed for link rewriting.
    let salt = match &import.backend {
        Some(_) => chevereto_variable(&chevereto, "crypt_salt")
            .await
            .expect("chevereto crypt_salt is missing, viewer links cannot be rewritten"),
        None => String::new(),
    };
    let id_padding = chevereto_variable(&chevereto, "id_padding")
        .await
        .map_or(0, |value| {
            value.parse().expect("chevereto id_padding is not a number")
        });
    let storage_urls: Vec<String> = sqlx::query_scalar("select storage_url from chv_storages")
        .fetch_all(&chevereto)
        .await
        .expect("failed to read chv_storages");
    let extensions: Vec<String> =
        sqlx::query_scalar("select distinct image_extension from chv_images where image_type = 1")
            .fetch_all(&chevereto)
            .await
            .expect("failed to read chevereto image extensions");

    let mut backend = match &import.backend {
        Some(backend_config) => {
            let mut backend = PgConnection::connect(&backend_config.database_url)
                .await
                .expect("failed to connect to backend database");
            chevereto_import::install_backend_functions(&mut backend)
                .await
                .expect("failed to install functions in backend database");
            Some(backend)
        }
        None => None,
    };

    let mut uploaders = Uploaders {
        lookup,
        fallback_username: import.fallback_username,
        fallback_user_id: None,
        by_username: HashMap::new(),
    };

    let link_settings = LinkSettings {
        public_url: import.chevereto.public_url,
        link_hosts: import.chevereto.link_hosts,
        elysia_public_url: import.elysia.public_url,
        salt,
        id_padding,
    };
    let link_pattern = chevereto_import::link_pattern(&link_settings, &storage_urls, &extensions);
    let context = Arc::new(Context {
        pool,
        storage,
        settings,
        link_settings,
        http_client: reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build http client"),
    });
    let semaphore = Arc::new(Semaphore::new(import.concurrency));
    let mut last_image_id = read_state(&import.state_file);
    prune_failures(&import.failed_file, last_image_id);
    let (mut imported, mut failed, mut rewritten_rows) = (0u64, 0u64, 0u64);

    // The state file only advances after a chunk is fully stored and its links
    // rewritten, so a crash replays at most that chunk: dedup returns the same
    // Elysia ids and the rewrite finds no Chevereto link left to change.
    loop {
        let rows: Vec<ChunkRow> = sqlx::query_as(CHUNK_QUERY)
            .bind(last_image_id)
            .bind(import.batch_size)
            .fetch_all(&chevereto)
            .await
            .expect("failed to read chevereto images");
        let Some(chunk_last_image_id) = rows.last().map(|row| row.0) else {
            break;
        };

        let mut tasks = JoinSet::new();
        for row in rows {
            let image = CheveretoImage {
                id: row.0,
                name: row.1,
                extension: row.2,
                storage_mode: row.3,
                date_folder: row.4,
                uploaded_at: row.5,
                original_filename: row.6,
                storage_url: row.7,
                uploader_username: row.8,
            };
            let uploader = uploaders
                .resolve(
                    &context.pool,
                    backend.as_mut(),
                    image.uploader_username.as_deref(),
                )
                .await;
            let context = context.clone();
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            tasks.spawn(async move {
                let result = import_image(&context, &image, uploader).await;
                drop(permit);
                result
            });
        }

        let mut rewrites = Vec::new();
        while let Some(result) = tasks.join_next().await {
            match result.expect("import task panicked") {
                Ok(image_rewrites) => {
                    imported += 1;
                    rewrites.extend(image_rewrites);
                }
                Err(failure) => {
                    failed += 1;
                    eprintln!("image {} failed: {}", failure.image_id, failure.error);
                    append_failure(&import.failed_file, &failure);
                }
            }
        }

        if let Some(backend) = backend.as_mut() {
            match chevereto_import::rewrite_links(backend, &link_pattern, &rewrites).await {
                Ok(updated) => rewritten_rows += updated,
                Err(error) => panic!(
                    "rewriting backend links for images {} to {chunk_last_image_id} failed, \
                     nothing was committed and the state file was not advanced \
                     (fix the cause, e.g. widen a too short column, and rerun): {error}",
                    last_image_id + 1
                ),
            }
        }
        last_image_id = chunk_last_image_id;
        write_state(&import.state_file, last_image_id);
        println!(
            "imported {imported}, failed {failed}, rewrote {rewritten_rows} rows, last image_id {last_image_id}"
        );
    }
    println!("done");
}
