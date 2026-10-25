//! Copy every image of a Chevereto v4 instance into Elysia and rewrite the
//! Chevereto links stored in a backend's database.
//!
//! Images go through the real upload pipeline with the size limits lifted,
//! keep their uploader and upload date, and are processed in the background
//! in chunks whose progress is saved in `chevereto_import`, so the import can
//! be stopped and started again.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use actix_web::web::Data;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::handlers::images::upload::UploadOptionsQuery;
use crate::repository::chevereto_import::{
    self as import_repo, CheveretoImportFailureRow, CheveretoImportRow,
};
use crate::repository::files as files_repo;
use crate::repository::users;
use crate::services::auth as auth_service;
use crate::services::files::{self, image_url, thumbnail_url};
use crate::services::{check_shutdown, paginate, track_job};
use crate::settings::ElysiaSettings;
use crate::storage::Storage;

const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const BOUNDARY: &str = "(?![A-Za-z0-9])";

/// One image row read from Chevereto.
pub struct CheveretoImage {
    pub id: u64,
    pub name: String,
    pub extension: String,
    pub storage_mode: String,
    /// Upload folder as "Y/m/d".
    pub date_folder: String,
    /// Upload time in unix seconds (GMT), `None` for zero or out of range dates.
    pub uploaded_at: Option<i64>,
    pub original_filename: Option<String>,
    pub storage_url: Option<String>,
    pub uploader_username: Option<String>,
}

/// Instance settings needed to fetch images and rewrite links.
pub struct LinkSettings {
    pub public_url: String,
    pub link_hosts: Vec<String>,
    pub elysia_public_url: String,
    pub salt: String,
    pub id_padding: u64,
}

/// Port of Chevereto's `cheveretoID()` encoding (salted, shuffled base 62).
pub fn encode_id(id: u64, salt: &str, padding: u64) -> String {
    let hash: String = Sha256::digest(salt.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut pairs: Vec<(u8, u8)> = ALPHABET.bytes().zip(hash.bytes()).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let digits: Vec<u8> = pairs.into_iter().map(|(character, _)| character).collect();

    let mut remaining = if padding > 0 { id * padding } else { id };
    let mut encoded = Vec::new();
    loop {
        encoded.push(digits[(remaining % 62) as usize]);
        remaining /= 62;
        if remaining == 0 {
            break;
        }
    }
    encoded.reverse();
    String::from_utf8(encoded).expect("alphabet is ascii")
}

/// Folder below the images base, or `None` for unsupported storage modes.
pub fn relative_folder(image: &CheveretoImage) -> Option<String> {
    match image.storage_mode.as_str() {
        "datefolder" => Some(format!("{}/", image.date_folder)),
        "direct" => Some(String::new()),
        "old" => Some("old/".to_string()),
        _ => None,
    }
}

fn with_trailing_slash(url: &str) -> String {
    format!("{}/", url.trim_end_matches('/'))
}

fn without_scheme(url: &str) -> &str {
    url.split_once("://").map_or(url, |(_, rest)| rest)
}

/// URL the original file can be downloaded from.
pub fn fetch_url(image: &CheveretoImage, settings: &LinkSettings) -> Option<String> {
    let folder = relative_folder(image)?;
    let base = match &image.storage_url {
        Some(storage_url) => with_trailing_slash(storage_url),
        None => format!("{}/images/", settings.public_url.trim_end_matches('/')),
    };
    Some(format!("{base}{folder}{}.{}", image.name, image.extension))
}

/// Backslash-escapes Postgres regex metacharacters.
fn escape_regex(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if "\\.^$|?*+()[]{}".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn alternatives<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    let escaped: Vec<String> = values.into_iter().map(escape_regex).collect();
    format!("(?:{})", escaped.join("|"))
}

/// Postgres regex matching every Chevereto file and viewer link. Groups: 1 file
/// link, 2 its key, 3 viewer link, 4 and 5 the two halves of its key. A key is
/// the link without scheme and slug, as `link_rewrites` returns it.
pub fn link_pattern(
    settings: &LinkSettings,
    storage_urls: &[String],
    extensions: &[String],
) -> String {
    let hosts = alternatives(settings.link_hosts.iter().map(String::as_str));
    let local_bases = settings
        .link_hosts
        .iter()
        .map(|host| format!("{host}/images/"));
    let storage_bases = storage_urls
        .iter()
        .map(|storage_url| with_trailing_slash(without_scheme(storage_url)));
    let file_bases: Vec<String> = local_bases.chain(storage_bases).collect();
    let file_bases = alternatives(file_bases.iter().map(String::as_str));
    let extensions = alternatives(extensions.iter().map(String::as_str));
    format!(
        "(https?://({file_bases}[^\\s\"'<>?#,()\\[\\]{{}}\\\\]*\\.{extensions})){BOUNDARY}\
         |(https?://({hosts}/image/)(?:[^/\\s\"'<>?#,{{}}\\\\]*\\.)?([A-Za-z0-9]+)){BOUNDARY}"
    )
}

/// `(key, replacement)` pairs for every known Chevereto link to this image
/// (original, thumbnail, medium and viewer page), `None` for unsupported
/// storage modes. Medium links get the thumbnail.
pub fn link_rewrites(
    image: &CheveretoImage,
    settings: &LinkSettings,
    elysia_id: &str,
    elysia_ext: &str,
    has_thumbnail: bool,
) -> Option<Vec<(String, String)>> {
    let folder = relative_folder(image)?;
    let bases = match &image.storage_url {
        Some(storage_url) => vec![with_trailing_slash(without_scheme(storage_url))],
        None => settings
            .link_hosts
            .iter()
            .map(|host| format!("{host}/images/"))
            .collect(),
    };
    let elysia = settings.elysia_public_url.trim_end_matches('/');
    let image_link = image_url(elysia, elysia_id, elysia_ext);
    let thumbnail_link = thumbnail_url(elysia, elysia_id, elysia_ext, has_thumbnail);
    let (name, extension) = (&image.name, &image.extension);
    let mut rewrites = Vec::new();
    for base in bases {
        let file = format!("{base}{folder}{name}");
        rewrites.push((format!("{file}.{extension}"), image_link.clone()));
        rewrites.push((format!("{file}.md.{extension}"), thumbnail_link.clone()));
        rewrites.push((format!("{file}.th.{extension}"), thumbnail_link.clone()));
    }
    let short_id = encode_id(image.id, &settings.salt, settings.id_padding);
    for host in &settings.link_hosts {
        rewrites.push((
            format!("{host}/image/{short_id}"),
            format!("{elysia}/i/{elysia_id}"),
        ));
    }
    Some(rewrites)
}

const TABLE_SQL: &str = r#"create temp table if not exists chevereto_links
     (link_key text primary key, replacement text not null) on commit delete rows"#;

const APPLY_FUNCTION_SQL: &str = r#"
create or replace function pg_temp.apply_link_rewrites(value text, pattern text)
returns text language plpgsql stable as $function$
declare
    pieces text[];
    link text[];
    result text;
    piece_index integer := 1;
begin
    if value is null or value !~ pattern then
        return value;
    end if;
    pieces := regexp_split_to_array(value, pattern);
    result := pieces[1];
    for link in select regexp_matches(value, pattern, 'g') loop
        piece_index := piece_index + 1;
        result := result || coalesce(
            (select replacement from pg_temp.chevereto_links
             where link_key = coalesce(link[2], link[4] || link[5])),
            link[1], link[3]
        ) || pieces[piece_index];
    end loop;
    return result;
end
$function$
"#;

const REWRITE_FUNCTION_SQL: &str = r#"
create or replace function pg_temp.rewrite_links(pattern text)
returns bigint language plpgsql as $function$
declare
    target record;
    assignments text;
    filters text;
    updated_rows bigint;
    total bigint := 0;
begin
    for target in
        select n.nspname as schema_name, c.relname as table_name,
               array_agg(a.attname order by a.attnum) as column_names,
               array_agg(format_type(a.atttypid, null) order by a.attnum) as column_types
        from pg_catalog.pg_attribute a
        join pg_catalog.pg_class c on c.oid = a.attrelid
        join pg_catalog.pg_namespace n on n.oid = c.relnamespace
        join pg_catalog.pg_type column_type on column_type.oid = a.atttypid
        join pg_catalog.pg_type base_type
          on base_type.oid = coalesce(nullif(column_type.typbasetype, 0), column_type.oid)
        left join pg_catalog.pg_type element_type
          on element_type.oid = base_type.typelem and base_type.typcategory = 'A'
        where c.relkind = 'r'
          and n.nspname not in ('pg_catalog', 'information_schema')
          and n.nspname not like 'pg\_toast%'
          and n.nspname not like 'pg\_temp%'
          and a.attnum > 0
          and not a.attisdropped
          and a.attgenerated = ''
          and (base_type.typcategory = 'S'
               or base_type.oid in ('json'::regtype, 'jsonb'::regtype)
               or element_type.typcategory = 'S')
        group by n.nspname, c.relname
    loop
        select string_agg(format('%I = pg_temp.apply_link_rewrites(%I::text, $1)::%s',
                                 column_name, column_name, column_type), ', '),
               string_agg(format('(%I::text ~ $1 and pg_temp.apply_link_rewrites(%I::text, $1) is distinct from %I::text)',
                                 column_name, column_name, column_name), ' or ')
        into assignments, filters
        from unnest(target.column_names, target.column_types) as target_columns(column_name, column_type);
        execute format('update only %I.%I set %s where %s',
                       target.schema_name, target.table_name, assignments, filters)
        using pattern;
        get diagnostics updated_rows = row_count;
        total := total + updated_rows;
    end loop;
    return total;
end
$function$
"#;

const FIND_USER_FUNCTION_SQL: &str = r#"
create or replace function pg_temp.find_user(
    table_name regclass, username_column text, subject_column text, email_column text, username text
)
returns table(subject text, email text) language plpgsql stable as $function$
begin
    return query execute format('select %I::text, %s from %s where %I = $1 limit 1',
        subject_column,
        case when email_column is null then 'null::text' else format('%I::text', email_column) end,
        table_name, username_column)
    using username;
end
$function$
"#;

/// Where the backend keeps its users.
#[derive(Deserialize, ToSchema)]
pub struct BackendUsers {
    /// Table name, optionally schema qualified (e.g. `public.users`).
    pub table: String,
    pub username_column: String,
    /// Column holding the user's SSO subject (the OIDC `sub` claim).
    pub subject_column: String,
    pub email_column: Option<String>,
}

/// Creates the session-temporary link table, rewrite and user lookup functions.
/// They only exist on this connection, so use one `PgConnection` for all later calls.
pub async fn install_backend_functions(connection: &mut PgConnection) -> Result<()> {
    for sql in [
        TABLE_SQL,
        APPLY_FUNCTION_SQL,
        REWRITE_FUNCTION_SQL,
        FIND_USER_FUNCTION_SQL,
    ] {
        sqlx::query(sql).execute(&mut *connection).await?;
    }
    Ok(())
}

/// `(sso subject, email)` of the backend user with this exact username.
pub async fn find_backend_user(
    connection: &mut PgConnection,
    users: &BackendUsers,
    username: &str,
) -> Result<Option<(String, Option<String>)>> {
    Ok(sqlx::query_as(
        r#"select subject, email from pg_temp.find_user($1::regclass, $2, $3, $4, $5)"#,
    )
    .bind(&users.table)
    .bind(&users.username_column)
    .bind(&users.subject_column)
    .bind(&users.email_column)
    .bind(username)
    .fetch_optional(connection)
    .await?)
}

/// Fills the link table, emptied again when the caller's transaction ends.
async fn load_rewrites(connection: &mut PgConnection, rewrites: &[(String, String)]) -> Result<()> {
    let (keys, replacements): (Vec<String>, Vec<String>) = rewrites.iter().cloned().unzip();
    sqlx::query(
        r#"insert into pg_temp.chevereto_links select * from unnest($1::text[], $2::text[])
         on conflict do nothing"#,
    )
    .bind(keys)
    .bind(replacements)
    .execute(connection)
    .await?;
    Ok(())
}

/// Rewrites the links in every text-like column of every table, one update
/// per table, in one transaction. Returns the number of changed rows.
pub async fn rewrite_links(
    connection: &mut PgConnection,
    pattern: &str,
    rewrites: &[(String, String)],
) -> Result<u64> {
    if rewrites.is_empty() {
        return Ok(0);
    }
    let mut transaction = connection.begin().await?;
    load_rewrites(&mut transaction, rewrites).await?;
    let updated: i64 = sqlx::query_scalar(r#"select pg_temp.rewrite_links($1)"#)
        .bind(pattern)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(updated as u64)
}

/// Applies the rewrites to one string, for checking the regex output.
pub async fn apply_link_rewrites(
    connection: &mut PgConnection,
    value: &str,
    pattern: &str,
    rewrites: &[(String, String)],
) -> Result<String> {
    let mut transaction = connection.begin().await?;
    load_rewrites(&mut transaction, rewrites).await?;
    let rewritten: String = sqlx::query_scalar(r#"select pg_temp.apply_link_rewrites($1, $2)"#)
        .bind(value)
        .bind(pattern)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.rollback().await?;
    Ok(rewritten)
}

/// Where a run starts.
#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheveretoImportStart {
    /// After the last fully processed chunk of the previous run.
    Resume,
    /// From the lowest failed image, replaying everything after it.
    RetryFailed,
    /// From the first image, forgetting every failure.
    Restart,
}

#[derive(Deserialize, ToSchema)]
pub struct CheveretoImportParams {
    /// `mysql://` or `mariadb://` url of the Chevereto database (default `chv_` table prefix).
    pub chevereto_database_url: String,
    /// Public base url of the Chevereto site, images are downloaded from it.
    pub chevereto_public_url: String,
    /// Hosts (with port if not default) whose Chevereto links get rewritten,
    /// aliases included. Empty uses the host of `chevereto_public_url`.
    #[serde(default)]
    pub link_hosts: Vec<String>,
    /// Postgres url of the backend database whose Chevereto links get
    /// rewritten in every text-like column. None rewrites nothing.
    pub backend_database_url: Option<String>,
    /// Match Chevereto uploaders to backend users by exact username, they
    /// become that backend's SSO users. Needs `backend_database_url` and
    /// `auth.sso`. None gives every image to `fallback_username`.
    pub backend_users: Option<BackendUsers>,
    /// Elysia user (created if missing) owning guest uploads and images of
    /// Chevereto users without a backend account.
    pub fallback_username: String,
    /// Images read per chunk; progress is saved and links rewritten once per chunk.
    pub batch_size: u32,
    /// Images downloaded and processed in parallel.
    pub concurrency: u32,
    pub start: CheveretoImportStart,
}

/// State of the current or last run.
#[derive(Serialize, ToSchema)]
pub struct CheveretoImportProgress {
    pub running: bool,
    /// Last Chevereto image id of the last fully processed chunk, where `resume` starts.
    pub last_image_id: i64,
    /// Images the current or last run had to process.
    pub total: i64,
    /// Images the current or last run processed, failed ones included.
    pub done: i64,
    pub imported: i64,
    /// Backend rows whose links the current or last run rewrote.
    pub rewritten_rows: i64,
    /// Images in the failures list.
    pub failed: i64,
    /// RFC 3339, UTC. None before the first run.
    pub started_at: Option<String>,
    /// RFC 3339, UTC. None while running, or when a restart interrupted the run.
    pub finished_at: Option<String>,
    /// Why the last run stopped early, if it did.
    pub error: Option<String>,
}

/// An image that could not be imported.
#[derive(Serialize, ToSchema)]
pub struct CheveretoImportFailure {
    pub image_id: i64,
    /// None for an unsupported storage mode.
    pub fetch_url: Option<String>,
    pub error: String,
}

// ponytail: one run at a time per process, the progress itself is in the database.
static RUNNING: AtomicBool = AtomicBool::new(false);
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

pub async fn progress(pool: &PgPool) -> Result<CheveretoImportProgress> {
    let CheveretoImportRow {
        last_image_id,
        total,
        done,
        imported,
        rewritten_rows,
        failed,
        started_at,
        finished_at,
        error,
    } = import_repo::get(pool).await?;
    Ok(CheveretoImportProgress {
        running: RUNNING.load(Ordering::SeqCst),
        last_image_id,
        total,
        done,
        imported,
        rewritten_rows,
        failed,
        started_at,
        finished_at,
        error,
    })
}

/// One page (1-based) of the failed images, by image id, with the total.
pub async fn list_failures(
    pool: &PgPool,
    page: NonZeroU32,
    page_size: u32,
) -> Result<(Vec<CheveretoImportFailure>, i64)> {
    let (rows, total) = paginate(
        page,
        page_size,
        |offset, limit| import_repo::failures_page(pool, offset, limit),
        import_repo::failures_count(pool),
        |row| row.total,
    )
    .await?;
    let failures = rows
        .into_iter()
        .map(
            |CheveretoImportFailureRow {
                 image_id,
                 fetch_url,
                 error,
                 ..
             }| CheveretoImportFailure {
                image_id,
                fetch_url,
                error,
            },
        )
        .collect();
    Ok((failures, total))
}

/// Ask the running import to stop once its current chunk is saved.
pub fn stop() -> Result<()> {
    if !RUNNING.load(Ordering::SeqCst) {
        return Err(Error::Conflict);
    }
    STOP_REQUESTED.store(true, Ordering::SeqCst);
    Ok(())
}

/// Check every service the import needs, then run it in the background. Fails
/// if one is already running or a service is unreachable or misconfigured.
pub async fn start(
    pool: PgPool,
    storage: Data<Storage>,
    config: &Config,
    settings: ElysiaSettings,
    mut params: CheveretoImportParams,
) -> Result<()> {
    if params.batch_size == 0 || params.concurrency == 0 {
        return Err(Error::BadRequest(
            "batch size and concurrency must be greater than 0".into(),
        ));
    }
    if params.fallback_username.trim().is_empty() {
        return Err(Error::BadRequest(
            "fallback username must not be empty".into(),
        ));
    }
    params.backend_database_url = params
        .backend_database_url
        .filter(|url| !url.trim().is_empty());
    if params.backend_database_url.is_some() && config.server.public_url.is_empty() {
        return Err(Error::BadRequest(
            "server.public_url must be set in config.yml: rewritten links are built from it".into(),
        ));
    }
    let lookup = match params.backend_users.take() {
        Some(_) if params.backend_database_url.is_none() => {
            return Err(Error::BadRequest(
                "matching uploaders needs the backend database url".into(),
            ));
        }
        Some(users) => Some(UserLookup {
            issuer: config
                .auth
                .sso
                .as_ref()
                .ok_or_else(|| {
                    Error::BadRequest(
                        "auth.sso must be configured in config.yml to match uploaders: \
                         they become SSO users"
                            .into(),
                    )
                })?
                .normalized_issuer()
                .to_string(),
            users,
        }),
        None => None,
    };
    params.link_hosts.retain(|host| !host.trim().is_empty());
    if params.link_hosts.is_empty() {
        let public_url = url::Url::parse(&params.chevereto_public_url)
            .ok()
            .filter(url::Url::has_host)
            .ok_or_else(|| Error::BadRequest("invalid Chevereto public url".into()))?;
        // Keeps a non default port, links stored in the backend include it.
        let host = &public_url[url::Position::BeforeHost..url::Position::AfterPort];
        params.link_hosts = vec![host.to_string()];
    }
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err(Error::Conflict);
    }
    STOP_REQUESTED.store(false, Ordering::SeqCst);
    let elysia_public_url = config.server.public_url.clone();
    let (checks_sender, checks_receiver) = tokio::sync::oneshot::channel();
    // Checks and import run in their own task so a panic, or the request being
    // dropped mid-checks, still clears `RUNNING` below.
    tokio::spawn(track_job(async move {
        let job_pool = pool.clone();
        let result = tokio::spawn(async move {
            match prepare(
                &job_pool,
                storage,
                settings,
                params,
                lookup,
                elysia_public_url,
            )
            .await
            {
                Ok(job) => {
                    let _ = checks_sender.send(Ok(()));
                    Some(job.run().await)
                }
                Err(error) => {
                    let _ = checks_sender.send(Err(error));
                    None
                }
            }
        })
        .await
        .unwrap_or(Some(Err(Error::Internal)));
        if let Some(result) = result {
            if let Err(error) = &result {
                tracing::error!("chevereto import failed: {error}");
            }
            let error = result.err().map(|error| error.to_string());
            if let Err(error) = import_repo::finish(&pool, error.as_deref()).await {
                tracing::error!("failed to save the chevereto import end: {error}");
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
    }));
    checks_receiver.await.unwrap_or(Err(Error::Internal))
}

/// Shared by every upload task.
struct Context {
    pool: PgPool,
    storage: Data<Storage>,
    settings: ElysiaSettings,
    link_settings: LinkSettings,
    http_client: reqwest::Client,
}

/// An image that could not be imported.
struct Failure {
    image_id: u64,
    fetch_url: Option<String>,
    error: String,
}

/// One chunk of images after the saved id, with storage and uploader.
const CHUNK_QUERY: &str = r#"select cast(i.image_id as unsigned), i.image_name, i.image_extension,
     cast(i.image_storage_mode as char), date_format(i.image_date, '%Y/%m/%d'),
     cast(nullif(unix_timestamp(i.image_date_gmt), 0) as signed), i.image_original_filename,
     s.storage_url, u.user_username
     from chv_images i
     left join chv_storages s on s.storage_id = i.image_storage_id
     left join chv_users u on u.user_id = i.image_user_id
     where i.image_type = 1 and i.image_id > ?
     order by i.image_id limit ?"#;

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

fn chevereto_error(error: sqlx::Error) -> Error {
    Error::BadRequest(format!("chevereto database: {error}"))
}

fn backend_error(error: Error) -> Error {
    Error::BadRequest(format!("backend database: {error}"))
}

/// Fails unless Chevereto uses the default paths the link patterns assume.
async fn check_chevereto_settings(chevereto: &MySqlPool) -> Result<()> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        r#"select setting_name, setting_value from chv_settings
         where setting_name in ('upload_image_path', 'route_image', 'root_route')"#,
    )
    .fetch_all(chevereto)
    .await
    .map_err(chevereto_error)?;
    let setting = |name: &str| {
        rows.iter()
            .find(|(setting_name, _)| setting_name == name)
            .and_then(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };
    if let Some(value) = setting("upload_image_path")
        && value != "images"
    {
        return Err(Error::BadRequest(format!(
            "chevereto upload_image_path is '{value}', only the default 'images' is supported"
        )));
    }
    if let Some(value) = setting("route_image")
        && value != "image"
    {
        return Err(Error::BadRequest(format!(
            "chevereto route_image is '{value}', only the default 'image' is supported"
        )));
    }
    if setting("root_route").as_deref() == Some("image") {
        return Err(Error::BadRequest(
            "chevereto root_route is 'image', which makes image links ambiguous and is not supported"
                .into(),
        ));
    }
    Ok(())
}

/// A trimmed `chv_variables` value, `None` when the variable is missing or empty.
async fn chevereto_variable(chevereto: &MySqlPool, name: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "select variable_value from chv_variables where variable_name = ?",
    )
    .bind(name)
    .fetch_optional(chevereto)
    .await
    .map_err(chevereto_error)?
    .flatten()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty()))
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
    ) -> Result<Uuid> {
        let (Some(username), Some(backend), Some(lookup)) = (username, backend, &self.lookup)
        else {
            return self.fallback(pool).await;
        };
        if let Some(user_id) = self.by_username.get(username) {
            return Ok(*user_id);
        }
        let user_id = match find_backend_user(backend, &lookup.users, username)
            .await
            .map_err(backend_error)?
        {
            Some((subject, email)) => {
                users::upsert_sso_user(pool, &lookup.issuer, &subject, username, email.as_deref())
                    .await?
                    .id
            }
            None => {
                tracing::info!(
                    "chevereto user '{username}' has no backend account, their images go to '{}'",
                    self.fallback_username
                );
                self.fallback(pool).await?
            }
        };
        self.by_username.insert(username.to_string(), user_id);
        Ok(user_id)
    }

    /// The fallback user, looked up or created on first use.
    async fn fallback(&mut self, pool: &PgPool) -> Result<Uuid> {
        if let Some(user_id) = self.fallback_user_id {
            return Ok(user_id);
        }
        let user = match users::find_by_username_or_email(pool, &self.fallback_username).await? {
            Some(user) => user,
            None => {
                let password_hash = auth_service::hash_password_async(nanoid::nanoid!(32)).await?;
                users::create_local_user(pool, &self.fallback_username, None, &password_hash, false)
                    .await?
            }
        };
        self.fallback_user_id = Some(user.id);
        Ok(user.id)
    }
}

/// Downloads one image, stores it and returns its link rewrites.
async fn import_image(
    context: &Context,
    image: &CheveretoImage,
    uploader: Uuid,
) -> std::result::Result<Vec<(String, String)>, Failure> {
    let fetch_url = fetch_url(image, &context.link_settings);
    let failure = |error: String| Failure {
        image_id: image.id,
        fetch_url: fetch_url.clone(),
        error,
    };
    let Some(url) = &fetch_url else {
        return Err(failure("unsupported storage mode".to_string()));
    };
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
    Ok(link_rewrites(
        image,
        &context.link_settings,
        &stored.id,
        &stored.ext,
        stored.has_thumbnail,
    )
    .unwrap_or_default())
}

/// A checked import, ready to run.
struct Job {
    chevereto: MySqlPool,
    backend: Option<PgConnection>,
    context: Arc<Context>,
    uploaders: Uploaders,
    link_pattern: String,
    last_image_id: i64,
    batch_size: u32,
    concurrency: u32,
}

/// Bounds each health check, so a dead host fails the start request quickly.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Connects to and checks every service the import needs (Chevereto database
/// and site, backend database and users table), then saves the run start.
async fn prepare(
    pool: &PgPool,
    storage: Data<Storage>,
    settings: ElysiaSettings,
    params: CheveretoImportParams,
    lookup: Option<UserLookup>,
    elysia_public_url: String,
) -> Result<Job> {
    let settings = ElysiaSettings {
        max_file_size_bytes: 0,
        max_width_pixels: 0,
        max_height_pixels: 0,
        ..settings
    };
    // The mysql driver speaks mariadb, it just has no scheme for it.
    let chevereto_database_url =
        params
            .chevereto_database_url
            .replacen("mariadb://", "mysql://", 1);
    let chevereto = MySqlPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(CHECK_TIMEOUT)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query(r#"set time_zone = '+00:00'"#)
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&chevereto_database_url)
        .await
        .map_err(chevereto_error)?;
    check_chevereto_settings(&chevereto).await?;
    // A wrong salt maps viewer links to other images, so never guess it. It is
    // only needed for link rewriting.
    let salt = match &params.backend_database_url {
        Some(_) => chevereto_variable(&chevereto, "crypt_salt")
            .await?
            .ok_or_else(|| {
                Error::BadRequest(
                    "chevereto crypt_salt is missing, viewer links cannot be rewritten".into(),
                )
            })?,
        None => String::new(),
    };
    let id_padding = match chevereto_variable(&chevereto, "id_padding").await? {
        Some(value) => value
            .parse()
            .map_err(|_| Error::BadRequest("chevereto id_padding is not a number".into()))?,
        None => 0,
    };
    let storage_urls: Vec<String> = sqlx::query_scalar(r#"select storage_url from chv_storages"#)
        .fetch_all(&chevereto)
        .await
        .map_err(chevereto_error)?;
    let extensions: Vec<String> = sqlx::query_scalar(
        r#"select distinct image_extension from chv_images where image_type = 1"#,
    )
    .fetch_all(&chevereto)
    .await
    .map_err(chevereto_error)?;

    let backend = match &params.backend_database_url {
        Some(database_url) => {
            let mut backend =
                tokio::time::timeout(CHECK_TIMEOUT, PgConnection::connect(database_url))
                    .await
                    .map_err(|_| {
                        Error::BadRequest("backend database: connection timed out".into())
                    })?
                    .map_err(|error| backend_error(error.into()))?;
            install_backend_functions(&mut backend)
                .await
                .map_err(backend_error)?;
            // A lookup that matches nobody still fails on a wrong table or column.
            if let Some(lookup) = &lookup {
                find_backend_user(&mut backend, &lookup.users, "")
                    .await
                    .map_err(|error| Error::BadRequest(format!("backend users table: {error}")))?;
            }
            Some(backend)
        }
        None => None,
    };

    let http_client = crate::config::http_client_builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|_| Error::Internal)?;
    let chevereto_site_status = http_client
        .get(&params.chevereto_public_url)
        .timeout(CHECK_TIMEOUT)
        .send()
        .await
        .map_err(|error| Error::BadRequest(format!("chevereto public url: {error}")))?
        .status();
    if !chevereto_site_status.is_success() {
        return Err(Error::BadRequest(format!(
            "chevereto public url returned status {}",
            chevereto_site_status.as_u16()
        )));
    }

    let last_image_id = match params.start {
        CheveretoImportStart::Resume => import_repo::get(pool).await?.last_image_id,
        CheveretoImportStart::RetryFailed => {
            import_repo::first_failed_image_id(pool)
                .await?
                .ok_or_else(|| Error::BadRequest("no failed image to retry".into()))?
                - 1
        }
        CheveretoImportStart::Restart => 0,
    };
    let total: i64 = sqlx::query_scalar(
        r#"select count(*) from chv_images where image_type = 1 and image_id > ?"#,
    )
    .bind(last_image_id)
    .fetch_one(&chevereto)
    .await
    .map_err(chevereto_error)?;
    import_repo::start(pool, last_image_id, total).await?;

    let link_settings = LinkSettings {
        public_url: params.chevereto_public_url,
        link_hosts: params.link_hosts,
        elysia_public_url,
        salt,
        id_padding,
    };
    Ok(Job {
        link_pattern: link_pattern(&link_settings, &storage_urls, &extensions),
        chevereto,
        backend,
        context: Arc::new(Context {
            pool: pool.clone(),
            storage,
            settings,
            link_settings,
            http_client,
        }),
        uploaders: Uploaders {
            lookup,
            fallback_username: params.fallback_username.trim().to_string(),
            fallback_user_id: None,
            by_username: HashMap::new(),
        },
        last_image_id,
        batch_size: params.batch_size,
        concurrency: params.concurrency,
    })
}

impl Job {
    async fn run(self) -> Result<()> {
        let Job {
            chevereto,
            mut backend,
            context,
            mut uploaders,
            link_pattern,
            mut last_image_id,
            batch_size,
            concurrency,
        } = self;
        let pool = &context.pool;
        let semaphore = Arc::new(Semaphore::new(concurrency as usize));

        // The saved id only advances after a chunk is fully stored and its links
        // rewritten, so an interruption replays at most that chunk: dedup returns
        // the same Elysia ids and the rewrite finds no Chevereto link left to change.
        loop {
            if STOP_REQUESTED.load(Ordering::SeqCst) {
                return Err(Error::BadRequest(
                    "stopped by an admin, resume to continue".into(),
                ));
            }
            check_shutdown().map_err(|_| {
                Error::BadRequest("interrupted by a server shutdown, resume to continue".into())
            })?;
            let rows: Vec<ChunkRow> = sqlx::query_as(CHUNK_QUERY)
                .bind(last_image_id)
                .bind(batch_size)
                .fetch_all(&chevereto)
                .await
                .map_err(chevereto_error)?;
            let Some(chunk_last_image_id) = rows.last().map(|row| row.0) else {
                return Ok(());
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
                    .resolve(pool, backend.as_mut(), image.uploader_username.as_deref())
                    .await?;
                let context = context.clone();
                let permit = semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| Error::Internal)?;
                tasks.spawn(async move {
                    let result = import_image(&context, &image, uploader).await;
                    drop(permit);
                    result
                });
            }

            let mut rewrites = Vec::new();
            while let Some(result) = tasks.join_next().await {
                match result.map_err(|_| Error::Internal)? {
                    Ok(image_rewrites) => {
                        import_repo::record_imported(pool).await?;
                        rewrites.extend(image_rewrites);
                    }
                    Err(failure) => {
                        tracing::warn!(
                            "chevereto image {} failed: {}",
                            failure.image_id,
                            failure.error
                        );
                        import_repo::record_failure(
                            pool,
                            failure.image_id as i64,
                            failure.fetch_url.as_deref(),
                            &failure.error,
                        )
                        .await?;
                    }
                }
            }

            let rewritten_rows = match backend.as_mut() {
            Some(backend) => rewrite_links(backend, &link_pattern, &rewrites)
                .await
                .map_err(|error| {
                    Error::BadRequest(format!(
                        "rewriting backend links for images {} to {chunk_last_image_id} failed, \
                         nothing was committed and the progress was not advanced \
                         (fix the cause, e.g. widen a too short column, and resume): {error}",
                        last_image_id + 1
                    ))
                })?,
            None => 0,
        };
            last_image_id = chunk_last_image_id as i64;
            import_repo::finish_chunk(pool, last_image_id, rewritten_rows as i64).await?;
        }
    }
}
