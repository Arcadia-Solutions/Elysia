use std::future::Future;
use std::pin::Pin;

use actix_web::{FromRequest, HttpRequest, dev::Payload, web::Data};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::repository::users::{self, AuthProvider};
use crate::services::sso::SharedSsoState;
use crate::services::{auth, sso as sso_service};

/// Authenticated caller: the bearer must be a JWT session token for a live,
/// unbanned user row.
pub struct AuthUser {
    pub user_id: Uuid,
    pub is_admin: bool,
}

impl AuthUser {
    /// Reject callers without admin rights. The settings endpoints use this.
    pub fn require_admin(&self) -> Result<()> {
        if self.is_admin {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
}

/// Pull the bearer token out of the request headers, if present.
fn bearer_token(request: &HttpRequest) -> Option<String> {
    request
        .headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(str::to_string)
}

impl FromRequest for AuthUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self>>>>;

    fn from_request(request: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let config = request
            .app_data::<Data<Config>>()
            .expect("Config not configured")
            .clone();
        let pool = request
            .app_data::<Data<PgPool>>()
            .expect("PgPool not configured")
            .clone();
        let sso_state = request.app_data::<SharedSsoState>().cloned();
        let token = bearer_token(request);
        Box::pin(async move {
            let token = token.ok_or(Error::Unauthorized)?;
            let claims = auth::decode_jwt(&token, &config.auth.jwt_secret)?;
            let user_id = claims.user_id()?;
            let row = users::find_by_id(pool.get_ref(), user_id)
                .await?
                .ok_or(Error::Unauthorized)?;
            if row.banned {
                return Err(Error::Unauthorized);
            }
            // SSO rows are only as live as the provider's status endpoint says
            // (the local banned flag is not set from there). Fail closed when
            // SSO got disabled or the row has no subject.
            if row.provider == AuthProvider::Sso {
                let (Some(sso), Some(sso_state), Some(subject)) = (
                    config.auth.sso.as_ref(),
                    sso_state,
                    row.external_sub.as_deref(),
                ) else {
                    return Err(Error::Unauthorized);
                };
                if !sso_service::user_active(&sso_state, sso, row.id, subject).await {
                    return Err(Error::Unauthorized);
                }
            }
            users::update_last_seen(pool.get_ref(), row.id).await?;
            Ok(AuthUser {
                user_id: row.id,
                is_admin: row.is_admin,
            })
        })
    }
}

/// Upload caller: a session user (see [`AuthUser`]), or the SSO provider
/// uploading on behalf of one of its users. The provider authenticates with
/// the SSO client credentials (`Authorization: Basic`) and names the user in
/// `On-Behalf-Of-Sub` (plus `On-Behalf-Of-Username` for first provisioning).
/// The user maps to the same shadow row an SSO login would. Only the upload
/// routes take this extractor, so the client secret opens nothing else.
pub struct UploadUser {
    pub user_id: Uuid,
}

/// Decoded `Authorization: Basic` credentials, if present.
fn basic_credentials(request: &HttpRequest) -> Option<(String, String)> {
    let encoded = request
        .headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Basic ")?;
    let decoded = String::from_utf8(STANDARD.decode(encoded).ok()?).ok()?;
    let (client_id, client_secret) = decoded.split_once(':')?;
    Some((client_id.to_string(), client_secret.to_string()))
}

fn header_value(request: &HttpRequest, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|header| header.to_str().ok())
        .map(str::to_string)
}

/// Compare secrets through their SHA-256, so the comparison time leaks
/// nothing about the expected value.
fn secrets_match(given: &str, expected: &str) -> bool {
    Sha256::digest(given.as_bytes()) == Sha256::digest(expected.as_bytes())
}

impl FromRequest for UploadUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self>>>>;

    fn from_request(request: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let Some((client_id, client_secret)) = basic_credentials(request) else {
            let session = AuthUser::from_request(request, payload);
            return Box::pin(async move {
                Ok(UploadUser {
                    user_id: session.await?.user_id,
                })
            });
        };
        let config = request
            .app_data::<Data<Config>>()
            .expect("Config not configured")
            .clone();
        let pool = request
            .app_data::<Data<PgPool>>()
            .expect("PgPool not configured")
            .clone();
        let subject = header_value(request, "On-Behalf-Of-Sub");
        let username = header_value(request, "On-Behalf-Of-Username").unwrap_or_default();
        Box::pin(async move {
            let sso = config.auth.sso.as_ref().ok_or(Error::Unauthorized)?;
            if !(client_id == sso.client_id && secrets_match(&client_secret, &sso.client_secret)) {
                return Err(Error::Unauthorized);
            }
            let subject = subject
                .filter(|subject| !subject.is_empty())
                .ok_or_else(|| Error::BadRequest("missing On-Behalf-Of-Sub header".into()))?;
            // The provider vouches for the user, so its status endpoint is not
            // asked again; a local ban still applies.
            let row = users::upsert_sso_user(
                pool.get_ref(),
                sso.normalized_issuer(),
                &subject,
                &username,
                None,
            )
            .await?;
            if row.banned {
                return Err(Error::Unauthorized);
            }
            users::update_last_seen(pool.get_ref(), row.id).await?;
            Ok(UploadUser { user_id: row.id })
        })
    }
}
