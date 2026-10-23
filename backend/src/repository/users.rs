use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::repository::files::SortOrder;
use crate::services::auth as auth_service;

/// Which credential backs a user row. Local rows hold an Argon2 hash;
/// SSO rows are JIT-provisioned from an external sign-on backend and hold
/// issuer/subject instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthProvider {
    Local,
    Sso,
}

impl AuthProvider {
    fn from_database(value: String) -> Self {
        match value.as_str() {
            "sso" => Self::Sso,
            _ => Self::Local,
        }
    }

    /// Database string form, for API responses.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Sso => "sso",
        }
    }
}

/// One user row with everything login and session checks need.
pub struct AuthUserRow {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub is_admin: bool,
    pub banned: bool,
    pub provider: AuthProvider,
    /// Provider subject of SSO rows, used for their live ban check.
    pub external_sub: Option<String>,
}

/// Raw user row straight from the database, before parsing the provider.
#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: Option<String>,
    password_hash: Option<String>,
    is_admin: bool,
    banned: bool,
    provider: String,
    external_sub: Option<String>,
}

impl UserRow {
    fn into_auth(self) -> AuthUserRow {
        AuthUserRow {
            id: self.id,
            username: self.username,
            email: self.email,
            password_hash: self.password_hash,
            is_admin: self.is_admin,
            banned: self.banned,
            provider: AuthProvider::from_database(self.provider),
            external_sub: self.external_sub,
        }
    }
}

/// Create a local (password) user. A username or email clash maps to
/// `Conflict` so handlers can return 409.
pub async fn create_local_user(
    pool: &PgPool,
    username: &str,
    email: Option<&str>,
    password_hash: &str,
    is_admin: bool,
) -> Result<AuthUserRow> {
    let row = sqlx::query_as!(
        UserRow,
        r#"insert into users (username, email, password_hash, is_admin)
           values ($1, $2, $3, $4)
           returning id, username, email, password_hash, is_admin, banned,
                     provider::text as "provider!", external_sub"#,
        username,
        email,
        password_hash,
        is_admin,
    )
    .fetch_one(pool)
    .await
    .map_err(|error| match is_unique_violation(&error) {
        true => Error::Conflict,
        false => Error::Db(error),
    })?;
    Ok(row.into_auth())
}

/// Find a user by username or email, for password login. A login containing
/// `@` is an email (usernames cannot contain one), otherwise a username, so
/// a username that collides with someone else's email never matches the
/// wrong row. Both sides are normalized the way rows are stored.
pub async fn find_by_username_or_email(pool: &PgPool, login: &str) -> Result<Option<AuthUserRow>> {
    let normalized = crate::services::auth::normalize_login(login);
    if normalized.contains('@') {
        Ok(sqlx::query_as!(
            UserRow,
            r#"select id, username, email, password_hash, is_admin, banned,
                      provider::text as "provider!", external_sub
               from users where email = $1 limit 1"#,
            normalized,
        )
        .fetch_optional(pool)
        .await?
        .map(UserRow::into_auth))
    } else {
        Ok(sqlx::query_as!(
            UserRow,
            r#"select id, username, email, password_hash, is_admin, banned,
                      provider::text as "provider!", external_sub
               from users where username = $1 limit 1"#,
            normalized,
        )
        .fetch_optional(pool)
        .await?
        .map(UserRow::into_auth))
    }
}

/// Find a user by id, for session validation.
pub async fn find_by_id(pool: &PgPool, user_id: Uuid) -> Result<Option<AuthUserRow>> {
    Ok(sqlx::query_as!(
        UserRow,
        r#"select id, username, email, password_hash, is_admin, banned,
                  provider::text as "provider!", external_sub
           from users where id = $1"#,
        user_id,
    )
    .fetch_optional(pool)
    .await?
    .map(UserRow::into_auth))
}

/// Replace the password hash of a local user, for password changes.
pub async fn update_password_hash(pool: &PgPool, user_id: Uuid, password_hash: &str) -> Result<()> {
    sqlx::query!(
        r#"update users set password_hash = $2 where id = $1"#,
        user_id,
        password_hash
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Ban or unban a user. Banned users fail every login and session check.
/// False when no user has this id.
pub async fn set_banned(pool: &PgPool, user_id: Uuid, banned: bool) -> Result<bool> {
    let result = sqlx::query!(
        r#"update users set banned = $2 where id = $1"#,
        user_id,
        banned
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Set or clear (`None` follows the global default) a user's storage quota
/// override. False when no user has this id.
pub async fn set_storage_quota(
    pool: &PgPool,
    user_id: Uuid,
    storage_quota_bytes: Option<i64>,
) -> Result<bool> {
    let result = sqlx::query!(
        r#"update users set storage_quota_bytes = $2 where id = $1"#,
        user_id,
        storage_quota_bytes
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// How much storage a user holds and may hold.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct StorageUsage {
    /// Bytes of the user's available and trashed files.
    pub used_bytes: i64,
    /// Effective quota in bytes; null is unlimited.
    pub quota_bytes: Option<i64>,
}

impl StorageUsage {
    /// Bytes the user may still store; `None` is unlimited.
    pub fn remaining_bytes(&self) -> Option<u64> {
        self.quota_bytes
            .map(|quota| quota.saturating_sub(self.used_bytes).max(0) as u64)
    }
}

/// A user's storage usage and effective quota: their override, else
/// `default_quota_bytes` (admins are unlimited without an override). 0 is unlimited.
// ponytail: concurrent uploads can overshoot the quota by the files in flight,
// lock the user row if that matters.
pub async fn storage_usage(
    pool: &PgPool,
    user_id: Uuid,
    default_quota_bytes: u64,
) -> Result<StorageUsage> {
    let row = sqlx::query!(
        r#"select coalesce(storage_quota_bytes, case when is_admin then 0 else $2 end) as "quota_bytes!",
                  (select coalesce(sum(size), 0) from files
                   where uploaded_by_user_id = users.id and status <> 'deleted')::bigint as "used_bytes!"
           from users where id = $1"#,
        user_id,
        default_quota_bytes as i64,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound)?;
    Ok(StorageUsage {
        used_bytes: row.used_bytes,
        quota_bytes: (row.quota_bytes > 0).then_some(row.quota_bytes),
    })
}

/// Stamp a user who just made an API request as seen now, and the API key it
/// authenticated with (if any) as used now.
pub async fn update_last_seen(
    pool: &PgPool,
    user_id: Uuid,
    api_key_id: Option<Uuid>,
) -> Result<()> {
    // One round trip stamps the user and, when the request used one, its API key.
    sqlx::query!(
        r#"with used_key as (update api_keys set last_used_at = now() where id = $2)
         update users set last_seen = now() where id = $1"#,
        user_id,
        api_key_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// User rows whose username or email contains `search` (ignoring case), all when unset.
pub async fn count_users(pool: &PgPool, search: Option<&str>) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"select count(*) as "count!" from users
           where $1::text is null
              or strpos(lower(username), lower($1)) > 0
              or strpos(lower(email), lower($1)) > 0"#,
        search
    )
    .fetch_one(pool)
    .await?)
}

/// Username deconfliction tries before giving up.
const USERNAME_ATTEMPTS: u32 = 10;

/// Find an SSO shadow row by its provider issuer + subject.
pub async fn find_by_issuer_sub(
    pool: &PgPool,
    issuer: &str,
    sub: &str,
) -> Result<Option<AuthUserRow>> {
    Ok(sqlx::query_as!(
        UserRow,
        r#"select id, username, email, password_hash, is_admin, banned,
                  provider::text as "provider!", external_sub
           from users where external_issuer = $1 and external_sub = $2"#,
        issuer,
        sub,
    )
    .fetch_optional(pool)
    .await?
    .map(UserRow::into_auth))
}

/// Find-or-create the SSO shadow row for a provider identity. Usernames are
/// deconflicted with a short suffix, clashing emails are dropped to NULL
/// (the column is unique, NULLs are allowed). Repeat logins reuse the row.
pub async fn upsert_sso_user(
    pool: &PgPool,
    issuer: &str,
    sub: &str,
    username: &str,
    email: Option<&str>,
) -> Result<AuthUserRow> {
    // A unique violation means a concurrent first login of the same identity,
    // or a concurrent signup taking the username or email: start over once.
    for _ in 0..2 {
        if let Some(row) = find_by_issuer_sub(pool, issuer, sub).await? {
            return Ok(row);
        }
        let username = free_username(pool, &sanitize_username(username, sub)).await?;
        let email = usable_email(pool, email).await?;
        match sqlx::query_as!(
            UserRow,
            r#"insert into users (provider, username, email, password_hash, external_issuer, external_sub)
               values ('sso', $1, $2, null, $3, $4)
               returning id, username, email, password_hash, is_admin, banned,
                         provider::text as "provider!", external_sub"#,
            username,
            email,
            issuer,
            sub,
        )
        .fetch_one(pool)
        .await
        {
            Ok(row) => return Ok(row.into_auth()),
            Err(error) if is_unique_violation(&error) => continue,
            Err(error) => return Err(Error::Db(error)),
        }
    }
    Err(Error::Internal)
}

/// Shape a provider username into a valid local one, falling back to a
/// subject-based name when nothing usable remains.
fn sanitize_username(username: &str, sub: &str) -> String {
    let cleaned = username_characters(username);
    if auth_service::valid_username(&cleaned) {
        return cleaned;
    }
    username_characters(&format!("sso-user-{sub}"))
}

/// Keep only the characters a local username allows, capped at 32.
fn username_characters(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '_' || *character == '-'
        })
        .take(32)
        .collect()
}

/// Append a short random suffix until the username is free.
async fn free_username(pool: &PgPool, base: &str) -> Result<String> {
    let stem: String = base.chars().take(27).collect();
    let mut candidate = stem.clone();
    for _ in 0..USERNAME_ATTEMPTS {
        let taken = sqlx::query_scalar!(
            r#"select exists(select 1 from users where username = $1) as "taken!""#,
            candidate,
        )
        .fetch_one(pool)
        .await?;
        if !taken {
            return Ok(candidate);
        }
        let suffix: String = Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(4)
            .collect();
        candidate = format!("{stem}-{suffix}");
    }
    Err(Error::Internal)
}

/// Normalize a provider email, dropping it when invalid or already taken by
/// another row.
async fn usable_email(pool: &PgPool, email: Option<&str>) -> Result<Option<String>> {
    let Some(email) = email else {
        return Ok(None);
    };
    let normalized = auth_service::normalize_email(email);
    if !auth_service::valid_email(&normalized) {
        return Ok(None);
    }
    let clash = find_by_username_or_email(pool, &normalized).await?;
    if clash.is_some() {
        return Ok(None);
    }
    Ok(Some(normalized))
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.is_unique_violation())
}

/// A user as the admin user list and profile page show it.
#[derive(Serialize, ToSchema)]
pub struct UserSummary {
    #[schema(value_type = String)]
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    /// `local` or `sso`.
    pub provider: String,
    pub is_admin: bool,
    pub banned: bool,
    /// Account creation time as a UTC ISO-8601 string.
    pub created_at: String,
    /// Last authenticated API request as a UTC ISO-8601 string; null if never.
    pub last_seen: Option<String>,
    /// Available (not trashed) files the user uploaded.
    pub uploads: i64,
    /// Bytes of the user's available and trashed files.
    pub storage_used_bytes: i64,
    /// Storage quota override in bytes: null follows the global default, 0 is unlimited.
    pub storage_quota_bytes: Option<i64>,
    /// Quota in force: the override, else the global default (admins are
    /// unlimited without an override). Null is unlimited.
    pub effective_storage_quota_bytes: Option<i64>,
}

/// Column the user list is sorted by.
#[derive(Clone, Copy, Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsersSortBy {
    Username,
    CreatedAt,
    LastSeen,
    Uploads,
    StorageUsedBytes,
}

impl UsersSortBy {
    fn as_sql(self) -> &'static str {
        match self {
            UsersSortBy::Username => "username",
            UsersSortBy::CreatedAt => "created_at",
            UsersSortBy::LastSeen => "last_seen",
            UsersSortBy::Uploads => "uploads",
            UsersSortBy::StorageUsedBytes => "storage_used_bytes",
        }
    }
}

/// User list query.
#[derive(Clone, Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UsersQuery {
    /// 1-based page number.
    #[param(value_type = i64, minimum = 1)]
    pub page: NonZeroU32,
    #[serde(deserialize_with = "crate::repository::page_size")]
    #[param(minimum = 1, maximum = 500)]
    pub page_size: u32,
    /// Only users whose username or email contains this, ignoring case.
    pub search: Option<String>,
    pub sort_by: UsersSortBy,
    pub order: SortOrder,
}

impl UsersQuery {
    /// The search term, unless blank.
    pub fn search(&self) -> Option<&str> {
        self.search
            .as_deref()
            .map(str::trim)
            .filter(|search| !search.is_empty())
    }
}

/// One page of the users matching the query, in its order; users never
/// seen sort last by `last_seen` either way. `id` breaks ties.
// ponytail: the upload counts scan every file per page, add an index on
// files.uploaded_by_user_id if it gets slow.
pub async fn list(
    pool: &PgPool,
    query: &UsersQuery,
    default_quota_bytes: u64,
) -> Result<Vec<UserSummary>> {
    Ok(sqlx::query_as!(
        UserSummary,
        r#"select users.id, username, email, provider::text as "provider!", is_admin, banned,
                  to_char(users.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as "created_at!",
                  to_char(last_seen at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as last_seen,
                  count(files.id) filter (where files.status = 'available') as "uploads!",
                  coalesce(sum(files.size), 0)::bigint as "storage_used_bytes!", storage_quota_bytes,
                  nullif(coalesce(storage_quota_bytes, case when is_admin then 0 else $6 end), 0) as effective_storage_quota_bytes
           from users
           left join files on files.uploaded_by_user_id = users.id and files.status <> 'deleted'
           where $1::text is null
              or strpos(lower(username), lower($1)) > 0
              or strpos(lower(email), lower($1)) > 0
           group by users.id
           order by
             case when $2 = 'username' and $3 then username end asc,
             case when $2 = 'username' and not $3 then username end desc,
             case when $2 = 'created_at' and $3 then users.created_at end asc,
             case when $2 = 'created_at' and not $3 then users.created_at end desc,
             case when $2 = 'last_seen' and $3 then last_seen end asc nulls last,
             case when $2 = 'last_seen' and not $3 then last_seen end desc nulls last,
             case when $2 = 'uploads' and $3 then count(files.id) filter (where files.status = 'available') end asc,
             case when $2 = 'uploads' and not $3 then count(files.id) filter (where files.status = 'available') end desc,
             case when $2 = 'storage_used_bytes' and $3 then sum(files.size) end asc nulls first,
             case when $2 = 'storage_used_bytes' and not $3 then sum(files.size) end desc nulls last,
             users.id
           offset $4 limit $5"#,
        query.search(),
        query.sort_by.as_sql(),
        matches!(query.order, SortOrder::Asc),
        (i64::from(query.page.get()) - 1) * i64::from(query.page_size),
        i64::from(query.page_size),
        default_quota_bytes as i64,
    )
    .fetch_all(pool)
    .await?)
}

/// One user's summary by id, resolving their quota against `default_quota_bytes`.
pub async fn find_summary(
    pool: &PgPool,
    user_id: Uuid,
    default_quota_bytes: u64,
) -> Result<Option<UserSummary>> {
    Ok(sqlx::query_as!(
        UserSummary,
        r#"select users.id, username, email, provider::text as "provider!", is_admin, banned,
                  to_char(users.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as "created_at!",
                  to_char(last_seen at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as last_seen,
                  count(files.id) filter (where files.status = 'available') as "uploads!",
                  coalesce(sum(files.size), 0)::bigint as "storage_used_bytes!", storage_quota_bytes,
                  nullif(coalesce(storage_quota_bytes, case when is_admin then 0 else $2 end), 0) as effective_storage_quota_bytes
           from users
           left join files on files.uploaded_by_user_id = users.id and files.status <> 'deleted'
           where users.id = $1
           group by users.id"#,
        user_id,
        default_quota_bytes as i64,
    )
    .fetch_optional(pool)
    .await?)
}

/// Id of the user with exactly this username.
pub async fn find_id_by_username(pool: &PgPool, username: &str) -> Result<Option<Uuid>> {
    Ok(
        sqlx::query_scalar!(r#"select id from users where username = $1"#, username)
            .fetch_optional(pool)
            .await?,
    )
}
