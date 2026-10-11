use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};

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
    let row = sqlx::query!(
        r#"insert into users (username, email, password_hash, is_admin)
           values ($1, $2, $3, $4)
           returning id, username, email, password_hash, is_admin, banned,
                     provider::text as "provider!""#,
        username,
        email,
        password_hash,
        is_admin,
    )
    .fetch_one(pool)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => {
            Error::Conflict
        }
        _ => Error::Db(error),
    })?;
    Ok(AuthUserRow {
        id: row.id,
        username: row.username,
        email: row.email,
        password_hash: row.password_hash,
        is_admin: row.is_admin,
        banned: row.banned,
        provider: AuthProvider::from_database(row.provider),
    })
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
                      provider::text as "provider!"
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
                      provider::text as "provider!"
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
    Ok(sqlx::query!(
        r#"select id, username, email, password_hash, is_admin, banned,
                  provider::text as "provider!"
           from users where id = $1"#,
        user_id,
    )
    .fetch_optional(pool)
    .await?
    .map(|row| AuthUserRow {
        id: row.id,
        username: row.username,
        email: row.email,
        password_hash: row.password_hash,
        is_admin: row.is_admin,
        banned: row.banned,
        provider: AuthProvider::from_database(row.provider),
    }))
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
