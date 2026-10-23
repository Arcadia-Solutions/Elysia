use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{Error, Result};

/// What an API key may do, the `api_key_scope` Postgres enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "api_key_scope", rename_all = "lowercase")]
pub enum ApiKeyScope {
    /// Only the upload routes.
    Upload,
    /// Every route a session reaches, except password change, API key management and admin powers.
    Full,
}

impl ApiKeyScope {
    /// True when a key with this scope may call a route requiring `required`.
    pub fn allows(self, required: ApiKeyScope) -> bool {
        self == ApiKeyScope::Full || self == required
    }
}

/// An API key as its owner sees it. The token itself is never stored.
#[derive(Serialize, ToSchema)]
pub struct ApiKeySummary {
    #[schema(value_type = String)]
    pub id: Uuid,
    pub name: String,
    pub scope: ApiKeyScope,
    /// First characters of the token, to tell keys apart.
    pub token_prefix: String,
    /// Creation time as a UTC ISO-8601 string.
    pub created_at: String,
    /// Expiry as a UTC ISO-8601 string; null never expires.
    pub expires_at: Option<String>,
    /// Last use as a UTC ISO-8601 string; null if never used.
    pub last_used_at: Option<String>,
    /// Revocation time as a UTC ISO-8601 string; null while not revoked.
    pub revoked_at: Option<String>,
    /// True when the key is neither revoked nor expired.
    pub active: bool,
    /// Files uploaded with the key, trashed and deleted ones included.
    pub uploads: i64,
}

/// A live key matched by its token: whose it is and what it may do.
pub struct ActiveApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub scope: ApiKeyScope,
}

/// Insert a key for `user_id`, expiring at `expires_at` (an ISO-8601 timestamp,
/// never when None). A name clash with one of the user's active keys maps to
/// `Conflict`.
pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
    scope: ApiKeyScope,
    token_hash: &str,
    token_prefix: &str,
    expires_at: Option<&str>,
) -> Result<ApiKeySummary> {
    // ponytail: two concurrent creates can both pass the name check; harmless
    // duplicate labels. A unique index can't express "not expired" (now() isn't immutable).
    sqlx::query_as!(
        ApiKeySummary,
        r#"insert into api_keys (user_id, name, scope, token_hash, token_prefix, expires_at)
           select $1, $2, $3, $4, $5, $6::text::timestamptz
           where not exists (
             select 1 from api_keys
             where user_id = $1 and name = $2 and revoked_at is null
               and (expires_at is null or expires_at > now())
           )
           returning id, name, scope as "scope: ApiKeyScope", token_prefix,
                     to_char(created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as "created_at!",
                     to_char(expires_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as expires_at,
                     null::text as last_used_at, null::text as revoked_at,
                     true as "active!", 0::bigint as "uploads!""#,
        user_id,
        name,
        scope as ApiKeyScope,
        token_hash,
        token_prefix,
        expires_at,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(Error::Conflict)
}

/// True when `timestamp` parses as an ISO-8601 timestamp in the future.
/// Unparsable input maps to `BadRequest`.
pub async fn is_future(pool: &PgPool, timestamp: &str) -> Result<bool> {
    sqlx::query_scalar!(
        r#"select $1::text::timestamptz > now() as "future!""#,
        timestamp
    )
    .fetch_one(pool)
    .await
    .map_err(|error| match &error {
        // Class 22: data exception (bad format, out of range).
        sqlx::Error::Database(database)
            if database.code().is_some_and(|code| code.starts_with("22")) =>
        {
            Error::BadRequest("expires_at must be an ISO-8601 timestamp".into())
        }
        _ => Error::Db(error),
    })
}

/// Every key of a user, revoked and expired ones included, newest first.
pub async fn list(pool: &PgPool, user_id: Uuid) -> Result<Vec<ApiKeySummary>> {
    Ok(sqlx::query_as!(
        ApiKeySummary,
        r#"select api_keys.id, name, scope as "scope: ApiKeyScope", token_prefix,
                  to_char(api_keys.created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as "created_at!",
                  to_char(expires_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as expires_at,
                  to_char(last_used_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as last_used_at,
                  to_char(revoked_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as revoked_at,
                  (revoked_at is null and (expires_at is null or expires_at > now())) as "active!",
                  count(files.id) as "uploads!"
           from api_keys
           left join files on files.api_key_id = api_keys.id
           where user_id = $1
           group by api_keys.id
           order by api_keys.created_at desc, api_keys.id"#,
        user_id,
    )
    .fetch_all(pool)
    .await?)
}

/// Find the active (not revoked, not expired) key with this token hash.
pub async fn find_active(pool: &PgPool, token_hash: &str) -> Result<Option<ActiveApiKey>> {
    Ok(sqlx::query_as!(
        ActiveApiKey,
        r#"select id, user_id, scope as "scope: ApiKeyScope" from api_keys
           where token_hash = $1 and revoked_at is null
             and (expires_at is null or expires_at > now())"#,
        token_hash,
    )
    .fetch_optional(pool)
    .await?)
}

/// Revoke one of the user's keys. False when they have no active key with this id.
pub async fn revoke(pool: &PgPool, user_id: Uuid, api_key_id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"update api_keys set revoked_at = now()
         where id = $1 and user_id = $2 and revoked_at is null
           and (expires_at is null or expires_at > now())"#,
        api_key_id,
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}
