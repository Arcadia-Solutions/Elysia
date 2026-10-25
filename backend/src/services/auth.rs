use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::repository::sessions;
use crate::services::sso::random_bytes;

/// Access token lifetime when remembered: 24 hours, then the client refreshes it.
const REMEMBERED_ACCESS_TOKEN_LIFETIME_SECONDS: u64 = 24 * 60 * 60;

/// Access token lifetime when not remembered: 15 minutes.
const ACCESS_TOKEN_LIFETIME_SECONDS: u64 = 15 * 60;

/// How long a session lives without a refresh: 60 days when remembered.
const REMEMBERED_SESSION_LIFETIME_SECONDS: i32 = 60 * 24 * 60 * 60;

/// How long a session lives without a refresh: 2 hours when not remembered.
const SESSION_LIFETIME_SECONDS: i32 = 2 * 60 * 60;

/// Session token claims. Admin rights are re-read from the database on every
/// request. The wire names are the standard JWT ones (`sub`/`exp`/`iat`) so
/// signature and expiry validation apply; the Rust names stay explicit.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    #[serde(rename = "sub")]
    pub subject: String,
    #[serde(rename = "exp")]
    pub expiration: u64,
    #[serde(rename = "iat")]
    pub issued_at: u64,
    /// The `sessions` row backing the token; deleting it revokes the token.
    #[serde(rename = "sid")]
    pub session_id: Uuid,
}

impl SessionClaims {
    /// The user id the token was minted for.
    pub fn user_id(&self) -> Result<Uuid> {
        Uuid::parse_str(&self.subject).map_err(|_| Error::Unauthorized)
    }
}

/// Current unix time in seconds.
pub(crate) fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs()
}

/// Hash a password with Argon2 and a fresh random salt. CPU-heavy: callers
/// on async runtimes must use `hash_password_async`.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| Error::Internal)
}

/// Offload `hash_password` to the blocking pool so Actix workers stay free.
pub async fn hash_password_async(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|_| Error::Internal)?
}

/// Password length bounds for local accounts.
pub const MINIMUM_PASSWORD_LENGTH: usize = 12;
pub const MAXIMUM_PASSWORD_LENGTH: usize = 120;

/// Maximum length of a username or email, to bound stored rows.
pub const MAXIMUM_LOGIN_LENGTH: usize = 254;

/// Usernames are 3-32 ASCII letters, digits, `_` or `-`.
pub fn valid_username(username: &str) -> bool {
    let length = username.chars().count();
    (3..=32).contains(&length)
        && username.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

/// Emails are lowercased at rest, so `Alice@Example.com` and
/// `alice@example.com` are the same account.
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Usernames stay case-sensitive but never keep surrounding whitespace.
pub fn normalize_username(username: &str) -> String {
    username.trim().to_string()
}

/// A login is an email when it contains `@` (usernames cannot contain one),
/// so normalize it the same way the stored value was normalized.
pub fn normalize_login(login: &str) -> String {
    let trimmed = login.trim();
    if trimmed.contains('@') {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

pub fn valid_email(email: &str) -> bool {
    if email.len() > MAXIMUM_LOGIN_LENGTH || email.contains(' ') {
        return false;
    }
    let mut parts = email.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty() && !domain.is_empty() && domain.contains('.')
        }
        _ => false,
    }
}

/// Reject passwords outside the length bounds, with the message for the response.
pub fn validate_password(password: &str) -> Result<()> {
    let length = password.chars().count();
    if length < MINIMUM_PASSWORD_LENGTH {
        return Err(Error::BadRequest(
            "password must be at least 12 characters".into(),
        ));
    }
    if length > MAXIMUM_PASSWORD_LENGTH {
        return Err(Error::BadRequest(
            "password must be at most 120 characters".into(),
        ));
    }
    Ok(())
}

/// A valid Argon2 hash used only to equalize login timing for unknown users
/// and SSO rows, which have no password hash to check against.
pub fn dummy_hash() -> &'static str {
    "$argon2id$v=19$m=19456,t=2,p=1$jPojWbnVBAX8ShW6rTqduA$KiXMlsmfe39aYNAKHYsWPSJS1+9EqnIOAHXwhEbDE1M"
}

/// Check a password against its stored hash. Unparseable hashes fail closed.
/// CPU-heavy: callers on async runtimes must use `verify_password_async`.
pub fn verify_password(password_hash: &str, password: &str) -> bool {
    let parsed = match PasswordHash::new(password_hash) {
        Ok(parsed) => parsed,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Offload `verify_password` to the blocking pool so Actix workers stay free.
pub async fn verify_password_async(password_hash: String, password: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(&password_hash, &password))
        .await
        .unwrap_or(false)
}

/// A short-lived access token plus the refresh token that renews it.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct SessionTokens {
    pub token: String,
    pub refresh_token: String,
}

/// Store a new session for a user and mint its tokens. A remembered session
/// survives 60 days without a refresh, otherwise 2 hours.
pub async fn start_session(
    pool: &PgPool,
    user_id: &Uuid,
    remember_me: bool,
    jwt_secret: &str,
) -> Result<SessionTokens> {
    let refresh_token = generate_refresh_token();
    let lifetime_seconds = if remember_me {
        REMEMBERED_SESSION_LIFETIME_SECONDS
    } else {
        SESSION_LIFETIME_SECONDS
    };
    let session_id = sessions::create(
        pool,
        *user_id,
        &hash_token(&refresh_token),
        remember_me,
        lifetime_seconds,
    )
    .await?;
    Ok(SessionTokens {
        token: mint_jwt(user_id, &session_id, remember_me, jwt_secret)?,
        refresh_token,
    })
}

/// Rotate a refresh token: the old one stops working, the session lives on
/// for its full lifetime and gets a fresh access token. Replaying a spent
/// refresh token revokes its session.
pub async fn refresh_session(
    pool: &PgPool,
    refresh_token: &str,
    jwt_secret: &str,
) -> Result<SessionTokens> {
    let refresh_token_hash = hash_token(refresh_token);
    let new_refresh_token = generate_refresh_token();
    let Some(session) = sessions::refresh(
        pool,
        &refresh_token_hash,
        &hash_token(&new_refresh_token),
        REMEMBERED_SESSION_LIFETIME_SECONDS,
        SESSION_LIFETIME_SECONDS,
    )
    .await?
    else {
        sessions::revoke_by_spent_refresh_token(pool, &refresh_token_hash).await?;
        return Err(Error::Unauthorized);
    };
    Ok(SessionTokens {
        token: mint_jwt(
            &session.user_id,
            &session.id,
            session.remember_me,
            jwt_secret,
        )?,
        refresh_token: new_refresh_token,
    })
}

/// 32 random bytes, base64url-encoded.
fn generate_refresh_token() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes::<32>())
}

/// Sign an access token, valid 24 hours when remembered, 15 minutes otherwise.
/// Only valid while its `sessions` row is live, see `start_session`.
fn mint_jwt(
    user_id: &Uuid,
    session_id: &Uuid,
    remember_me: bool,
    jwt_secret: &str,
) -> Result<String> {
    let issued_at = now_seconds();
    let lifetime_seconds = if remember_me {
        REMEMBERED_ACCESS_TOKEN_LIFETIME_SECONDS
    } else {
        ACCESS_TOKEN_LIFETIME_SECONDS
    };
    let claims = SessionClaims {
        subject: user_id.to_string(),
        expiration: issued_at + lifetime_seconds,
        issued_at,
        session_id: *session_id,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|_| Error::Internal)
}

/// Decode and validate a session token (signature + expiry). Anything wrong
/// with the token is an `Unauthorized`, never a server error.
pub fn decode_jwt(token: &str, jwt_secret: &str) -> Result<SessionClaims> {
    decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|valid| valid.claims)
    .map_err(|_| Error::Unauthorized)
}

/// Start of every API key token, telling them apart from session tokens.
pub const API_KEY_TOKEN_PREFIX: &str = "elysia_";

/// Characters of a token kept as its displayed prefix.
const API_KEY_DISPLAYED_PREFIX_LENGTH: usize = 16;

/// A freshly generated API key: the token shown once, plus what gets stored.
pub struct GeneratedApiKey {
    pub token: String,
    pub token_hash: String,
    pub token_prefix: String,
}

/// A fresh API key token: the prefix plus 32 random bytes, base64url-encoded.
pub fn generate_api_key() -> GeneratedApiKey {
    let token = format!(
        "{API_KEY_TOKEN_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(random_bytes::<32>())
    );
    GeneratedApiKey {
        token_hash: hash_token(&token),
        token_prefix: token[..API_KEY_DISPLAYED_PREFIX_LENGTH].to_string(),
        token,
    }
}

/// SHA-256 (hex) of an API key or refresh token, as stored.
pub fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
