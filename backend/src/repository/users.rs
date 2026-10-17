use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};
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
        "update users set password_hash = $2 where id = $1",
        user_id,
        password_hash
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Ban or unban a user. Banned users fail every login and session check.
pub async fn set_banned(pool: &PgPool, user_id: Uuid, banned: bool) -> Result<()> {
    sqlx::query!(
        "update users set banned = $2 where id = $1",
        user_id,
        banned
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Stamp a successful login. Failures are ignored by callers on purpose.
pub async fn update_last_seen(pool: &PgPool, user_id: Uuid) -> Result<()> {
    sqlx::query!("update users set last_seen = now() where id = $1", user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Total user rows.
pub async fn count_users(pool: &PgPool) -> Result<i64> {
    Ok(
        sqlx::query_scalar!(r#"select count(*) as "count!" from users"#)
            .fetch_one(pool)
            .await?,
    )
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
            "select exists(select 1 from users where username = $1) as \"taken!\"",
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
