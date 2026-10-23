//! Finding images no longer linked from an application's database.
//!
//! A scan runs in the background, one select per table of the scanned
//! database, and stores its result in `unreferenced_files`, which is then
//! browsed page by page and trashed in the background too.

use std::collections::HashSet;
use std::future::Future;
use std::num::NonZeroU32;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::web::Data;
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use sqlx::{AnyConnection, AssertSqlSafe, Connection, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::repository::files::{self, ImageListRow, ImagesSortBy, SortOrder};
use crate::services::{files as files_service, paginate};
use crate::storage::Storage;

/// Unreferenced images returned per page.
pub const UNREFERENCED_PER_PAGE: i64 = 50;
/// Images trashed per batch by `start_trash`.
const TRASH_BATCH: usize = 500;
/// Age in hours past which `start_trash` refuses the scan result, as links
/// added since may point to its images. Also in the frontend help string.
const MAX_SCAN_AGE_HOURS: i32 = 24;

/// Text-like columns of every base table, ordered by table: table and column
/// names, then the same quoted by the database itself, the table with its schema.
const POSTGRES_COLUMNS_SQL: &str = r#"select c.table_name::text, c.column_name::text,
            quote_ident(c.table_schema::text) || '.' || quote_ident(c.table_name::text),
            quote_ident(c.column_name::text)
     from information_schema.columns c
     join information_schema.tables t
       on t.table_schema = c.table_schema and t.table_name = c.table_name
     where t.table_type = 'BASE TABLE'
       and c.table_schema not in ('pg_catalog', 'information_schema')
       and (c.data_type in ('text', 'character varying', 'character', 'json', 'jsonb', 'xml', 'ARRAY')
         or c.data_type = 'USER-DEFINED' and c.udt_name in ('citext', 'hstore'))
     order by c.table_schema, c.table_name, c.ordinal_position"#;

/// `POSTGRES_COLUMNS_SQL` for MySQL and MariaDB, on the connected database.
const MYSQL_COLUMNS_SQL: &str = r#"select cast(c.table_name as char), cast(c.column_name as char),
            cast(concat('`', replace(c.table_schema, '`', '``'), '`.`',
              replace(c.table_name, '`', '``'), '`') as char),
            cast(concat('`', replace(c.column_name, '`', '``'), '`') as char)
     from information_schema.columns c
     join information_schema.tables t
       on t.table_schema = c.table_schema and t.table_name = c.table_name
     where t.table_type = 'BASE TABLE' and c.table_schema = database()
       and c.data_type in ('char', 'varchar', 'tinytext', 'text', 'mediumtext', 'longtext', 'json')
     order by c.table_schema, c.table_name, c.ordinal_position"#;

/// Bound how long each select may run (10 minutes), so a huge table cannot
/// hold the scanned database for hours. MySQL and MariaDB name it differently.
const POSTGRES_TIMEOUT_SQL: &str = r#"set statement_timeout = '600s'"#;
const MYSQL_TIMEOUT_SQL: &str = r#"set session max_execution_time = 600000"#;
const MARIADB_TIMEOUT_SQL: &str = r#"set session max_statement_time = 600"#;

#[derive(Deserialize, ToSchema)]
pub struct ScanParams {
    /// `postgres://`, `postgresql://`, `mysql://` or `mariadb://` url of the
    /// database holding the image links.
    pub database_url: String,
    /// Hosts the links use, with an optional path prefix, e.g. `img.example.com`
    /// or `example.com/elysia`. A scheme and trailing slash are ignored.
    pub hosts: Vec<String>,
    /// Skip these: `table` for all its text-like columns, or `table.column`.
    /// Empty scans every text-like column.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// What a maintenance run does.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnreferencedTask {
    /// Scan a database for links, replacing the unreferenced images list.
    Scan,
    /// Trash every image of the unreferenced images list.
    Trash,
}

/// State of the current or last scan or trash run.
#[derive(Clone, Serialize, ToSchema)]
pub struct UnreferencedProgress {
    pub running: bool,
    pub task: UnreferencedTask,
    /// Tables scanned, or images trashed, so far.
    pub done: usize,
    /// Tables to scan, or images to trash.
    pub total: usize,
    /// Why the last run stopped early, if it did.
    pub error: Option<String>,
    /// Text-like columns read by the last finished scan.
    pub scanned_columns: usize,
    /// Distinct image ids linked from the database by the last finished scan.
    pub referenced_images: usize,
}

// ponytail: one run at a time per process, results reset on restart (the
// list itself is in the database).
static PROGRESS: Mutex<UnreferencedProgress> = Mutex::new(UnreferencedProgress {
    running: false,
    task: UnreferencedTask::Scan,
    done: 0,
    total: 0,
    error: None,
    scanned_columns: 0,
    referenced_images: 0,
});

fn update(change: impl FnOnce(&mut UnreferencedProgress)) {
    change(&mut PROGRESS.lock().expect("maintenance lock poisoned"));
}

pub fn progress() -> UnreferencedProgress {
    PROGRESS.lock().expect("maintenance lock poisoned").clone()
}

/// Run `job` in the background. Fails if a run is already going.
fn spawn(
    task: UnreferencedTask,
    job: impl Future<Output = Result<()>> + Send + 'static,
) -> Result<()> {
    {
        let mut progress = PROGRESS.lock().expect("maintenance lock poisoned");
        if progress.running {
            return Err(Error::Conflict);
        }
        progress.running = true;
        progress.task = task;
        progress.done = 0;
        progress.total = 0;
        progress.error = None;
        if let UnreferencedTask::Scan = task {
            progress.scanned_columns = 0;
            progress.referenced_images = 0;
        }
    }
    tokio::spawn(async move {
        // Run in its own task so a panic still clears `running` below.
        let result = tokio::spawn(job).await.unwrap_or(Err(Error::Internal));
        if let Err(error) = &result {
            tracing::error!("unreferenced images {task:?} failed: {error}");
        }
        update(|progress| {
            progress.running = false;
            progress.error = result.err().map(|error| error.to_string());
        });
    });
    Ok(())
}

#[derive(Clone, Copy)]
enum Dialect {
    Postgres,
    Mysql,
}

impl Dialect {
    /// One select reading all of `table`'s columns at once, joined by spaces,
    /// from the rows where any of them, lowercased, is `like` one of the
    /// `patterns` bound lowercase patterns. MySQL needs them bound once per column.
    fn table_statement(self, table: &Table, patterns: usize) -> String {
        let columns: Vec<String> = table
            .columns
            .iter()
            .map(|column| match self {
                Dialect::Postgres => format!("{column}::text"),
                Dialect::Mysql => format!("cast({column} as char)"),
            })
            .collect();
        let conditions = columns
            .iter()
            .flat_map(|column| {
                (1..=patterns).map(move |number| match self {
                    Dialect::Postgres => format!("lower({column}) like ${number}"),
                    Dialect::Mysql => format!("lower({column}) like ?"),
                })
            })
            .collect::<Vec<_>>()
            .join(" or ");
        // array_to_string over concat_ws: Postgres functions take at most 100 arguments.
        let joined = match self {
            Dialect::Postgres => format!("array_to_string(array[{}], ' ')", columns.join(", ")),
            Dialect::Mysql => format!("concat_ws(' ', {})", columns.join(", ")),
        };
        format!("select {joined} from {} where {conditions}", table.name)
    }
}

/// A table of the scanned database with the text-like columns to read, all
/// quoted by the database itself (the table with its schema).
struct Table {
    name: String,
    columns: Vec<String>,
}

/// Whether an `exclude` entry (`table` or `table.column`) selects the column.
fn selects(entry: &str, table: &str, column: &str) -> bool {
    match entry.split_once('.') {
        Some((entry_table, entry_column)) => entry_table == table && entry_column == column,
        None => entry == table,
    }
}

/// Adds the ids of every `{host}/i/<id>` and `{host}/t/<id>` link in `text`
/// for each of the lowercase `hosts`: image, thumbnail and viewer links, also
/// JSON-escaped (`\/`) or URL-encoded (`%2F`). Hosts are matched ignoring case.
pub fn extract_ids(text: &str, hosts: &[String], ids: &mut HashSet<String>) {
    let mut text = text.replace("%2F", "/").replace("%2f", "/");
    // Postgres array and hstore output escape backslashes again, turning a
    // JSON-escaped `\/` into `\\/`: undo every level.
    while text.contains("\\\\") {
        text = text.replace("\\\\", "\\");
    }
    let text = text.replace("\\/", "/");
    // Ids are case-sensitive, so only the search ignores case: ASCII
    // lowercasing keeps byte offsets.
    let lowercase_text = text.to_ascii_lowercase();
    for host in hosts {
        for (start, _) in lowercase_text.match_indices(host.as_str()) {
            let rest = &text[start + host.len()..];
            let Some(rest) = rest
                .strip_prefix("/i/")
                .or_else(|| rest.strip_prefix("/t/"))
            else {
                continue;
            };
            let id: String = rest
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect();
            if !id.is_empty() {
                ids.insert(id);
            }
        }
    }
}

fn external_database_error(error: sqlx::Error) -> Error {
    Error::BadRequest(format!("external database: {error}"))
}

/// The scanned database's text-like columns grouped by table, without the
/// `exclude` ones. Fails on an `exclude` entry matching no column, so a typo
/// does not silently scan what it meant to skip.
async fn tables(
    connection: &mut AnyConnection,
    dialect: Dialect,
    exclude: &[String],
) -> Result<Vec<Table>> {
    let sql = match dialect {
        Dialect::Postgres => POSTGRES_COLUMNS_SQL,
        Dialect::Mysql => MYSQL_COLUMNS_SQL,
    };
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(sql)
        .fetch_all(connection)
        .await
        .map_err(external_database_error)?;
    if let Some(entry) = exclude.iter().find(|entry| {
        !rows
            .iter()
            .any(|(table, column, _, _)| selects(entry, table, column))
    }) {
        return Err(Error::BadRequest(format!(
            "no text-like column matches {entry}"
        )));
    }
    let mut tables: Vec<Table> = Vec::new();
    for (table_name, column_name, quoted_table, quoted_column) in rows {
        if exclude
            .iter()
            .any(|entry| selects(entry, &table_name, &column_name))
        {
            continue;
        }
        match tables.last_mut() {
            Some(table) if table.name == quoted_table => table.columns.push(quoted_column),
            _ => tables.push(Table {
                name: quoted_table,
                columns: vec![quoted_column],
            }),
        }
    }
    Ok(tables)
}

async fn set_statement_timeout(connection: &mut AnyConnection, dialect: Dialect) -> Result<()> {
    let result = match dialect {
        Dialect::Postgres => {
            sqlx::raw_sql(POSTGRES_TIMEOUT_SQL)
                .execute(connection)
                .await
        }
        Dialect::Mysql => match sqlx::raw_sql(MYSQL_TIMEOUT_SQL)
            .execute(&mut *connection)
            .await
        {
            Ok(done) => Ok(done),
            Err(_) => sqlx::raw_sql(MARIADB_TIMEOUT_SQL).execute(connection).await,
        },
    };
    result.map(|_| ()).map_err(external_database_error)
}

/// Start scanning the database at `params.database_url` for links under
/// `params.hosts` (any scheme) in the background. On success the available
/// images uploaded before the scan and linked from nowhere replace the
/// unreferenced images list. Fails if a run is already going.
pub fn start_scan(pool: PgPool, params: ScanParams) -> Result<()> {
    let dialect = match params
        .database_url
        .split_once("://")
        .map(|(scheme, _)| scheme)
    {
        Some("postgres" | "postgresql") => Dialect::Postgres,
        Some("mysql" | "mariadb") => Dialect::Mysql,
        _ => {
            return Err(Error::BadRequest(
                "the database url must start with postgres://, postgresql://, mysql:// or mariadb://"
                    .into(),
            ));
        }
    };
    let mut hosts: Vec<String> = params
        .hosts
        .iter()
        .map(|host| {
            host.trim()
                .split_once("://")
                .map_or(host.trim(), |(_, rest)| rest)
                .trim_end_matches('/')
                .to_ascii_lowercase()
        })
        .collect();
    hosts.sort();
    hosts.dedup();
    if hosts.is_empty() || hosts.iter().any(String::is_empty) {
        return Err(Error::BadRequest("hosts must not be empty".into()));
    }
    spawn(UnreferencedTask::Scan, async move {
        scan(&pool, &hosts, dialect, &params).await
    })
}

async fn scan(
    pool: &PgPool,
    hosts: &[String],
    dialect: Dialect,
    params: &ScanParams,
) -> Result<()> {
    // Images uploaded from here on may be linked from rows already read.
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Internal)?
        .as_secs_f64();
    sqlx::any::install_default_drivers();
    // The `any` driver has no mariadb scheme, its mysql driver speaks it.
    let database_url = params.database_url.replacen("mariadb://", "mysql://", 1);
    let mut connection = AnyConnection::connect(&database_url)
        .await
        .map_err(external_database_error)?;
    set_statement_timeout(&mut connection, dialect).await?;
    let tables = tables(&mut connection, dialect, &params.exclude).await?;
    update(|progress| progress.total = tables.len());
    // Not `{host}/`: the slash may be escaped, see `extract_ids`.
    let patterns: Vec<String> = hosts.iter().map(|host| format!("%{host}%")).collect();
    let repeats = |table: &Table| match dialect {
        Dialect::Postgres => 1,
        Dialect::Mysql => table.columns.len(),
    };
    let mut ids = HashSet::new();
    for (index, table) in tables.iter().enumerate() {
        // Identifiers quoted by the database itself, the pattern bound.
        let mut query = sqlx::query_scalar::<_, String>(AssertSqlSafe(
            dialect.table_statement(table, patterns.len()),
        ));
        for _ in 0..repeats(table) {
            for pattern in &patterns {
                query = query.bind(pattern);
            }
        }
        let mut values = query.fetch(&mut connection);
        while let Some(value) = values.try_next().await.map_err(|error| {
            Error::BadRequest(format!("external database, table {}: {error}", table.name))
        })? {
            extract_ids(&value, hosts, &mut ids);
        }
        update(|progress| progress.done = index + 1);
    }
    let referenced: Vec<String> = ids.into_iter().collect();
    files::replace_unreferenced(pool, &referenced, started).await?;
    let scanned_columns = tables.iter().map(|table| table.columns.len()).sum();
    update(|progress| {
        progress.scanned_columns = scanned_columns;
        progress.referenced_images = referenced.len();
    });
    Ok(())
}

/// Start trashing every image of the unreferenced images list in the
/// background, as `admin_id`. Fails if a run is already going or the scan
/// is older than `MAX_SCAN_AGE_HOURS`.
pub async fn start_trash(pool: PgPool, storage: Data<Storage>, admin_id: Uuid) -> Result<()> {
    if let Some(scan) = files::unreferenced_scan(&pool, MAX_SCAN_AGE_HOURS).await?
        && !scan.recent
    {
        return Err(Error::BadRequest(format!(
            "the last scan is more than {MAX_SCAN_AGE_HOURS} hours old, scan again first"
        )));
    }
    spawn(UnreferencedTask::Trash, async move {
        let ids = files::unreferenced_ids(&pool).await?;
        update(|progress| progress.total = ids.len());
        for batch in ids.chunks(TRASH_BATCH) {
            files_service::trash(&pool, &storage, admin_id, true, batch).await?;
            update(|progress| progress.done += batch.len());
        }
        Ok(())
    })
}

/// One page (1-based) of the unreferenced images list, still available, in
/// the given order, with the total and when the scan started (RFC 3339, UTC).
pub async fn list_unreferenced(
    pool: &PgPool,
    page: NonZeroU32,
    page_size: u32,
    sort_by: ImagesSortBy,
    order: SortOrder,
) -> Result<(Vec<ImageListRow>, i64, Option<String>)> {
    let (rows, total) = paginate(
        page,
        page_size,
        |offset, limit| files::unreferenced_page(pool, offset, limit, sort_by, order),
        files::unreferenced_count(pool),
        |row| row.total,
    )
    .await?;
    let scanned_at = files::unreferenced_scan(pool, MAX_SCAN_AGE_HOURS)
        .await?
        .map(|scan| scan.scanned_at);
    Ok((rows, total, scanned_at))
}
