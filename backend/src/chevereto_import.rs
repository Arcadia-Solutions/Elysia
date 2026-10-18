//! Pure helpers for importing a Chevereto instance: ID encoding, source URLs,
//! backend user lookup and backend link rewriting.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

use crate::error::Result;
use crate::services::files::{image_url, thumbnail_url};

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

const TABLE_SQL: &str = "create temp table if not exists chevereto_links \
     (link_key text primary key, replacement text not null) on commit delete rows";

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

/// Where the backend keeps its users, see `src/bin/import_chevereto/import.example.yml`.
#[derive(Deserialize)]
pub struct BackendUsers {
    pub table: String,
    pub username_column: String,
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
    Ok(
        sqlx::query_as(
            "select subject, email from pg_temp.find_user($1::regclass, $2, $3, $4, $5)",
        )
        .bind(&users.table)
        .bind(&users.username_column)
        .bind(&users.subject_column)
        .bind(&users.email_column)
        .bind(username)
        .fetch_optional(connection)
        .await?,
    )
}

/// Fills the link table, emptied again when the caller's transaction ends.
async fn load_rewrites(connection: &mut PgConnection, rewrites: &[(String, String)]) -> Result<()> {
    let (keys, replacements): (Vec<String>, Vec<String>) = rewrites.iter().cloned().unzip();
    sqlx::query(
        "insert into pg_temp.chevereto_links select * from unnest($1::text[], $2::text[]) \
         on conflict do nothing",
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
    let updated: i64 = sqlx::query_scalar("select pg_temp.rewrite_links($1)")
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
    let rewritten: String = sqlx::query_scalar("select pg_temp.apply_link_rewrites($1, $2)")
        .bind(value)
        .bind(pattern)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.rollback().await?;
    Ok(rewritten)
}
