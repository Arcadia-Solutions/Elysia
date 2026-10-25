use std::future::Future;
use std::pin::Pin;

use actix_web::{FromRequest, HttpRequest, dev::Payload, web::Data};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::middlewares::RateLimits;
use crate::middlewares::rate_limit::FailedAuthLimit;
use crate::repository::api_keys::{self, ApiKeyScope};
use crate::repository::users::{self, AuthProvider};
use crate::services::sso::SharedSsoState;
use crate::services::{auth, sso as sso_service};

/// Authenticated caller: the bearer must be a JWT session token or a
/// `full` scope API key, of a live, unbanned user row.
pub struct AuthUser {
    pub user_id: Uuid,
    /// Always false for API key callers.
    pub is_admin: bool,
    /// The API key the request authenticated with; None for a session.
    pub api_key_id: Option<Uuid>,
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

    /// Reject API key callers, so a leaked key cannot change the password or
    /// mint and revoke keys.
    pub fn require_session(&self) -> Result<()> {
        match self.api_key_id {
            None => Ok(()),
            Some(_) => Err(Error::Forbidden),
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

fn config_and_pool(request: &HttpRequest) -> (Data<Config>, Data<PgPool>) {
    let config = request
        .app_data::<Data<Config>>()
        .expect("Config not configured")
        .clone();
    let pool = request
        .app_data::<Data<PgPool>>()
        .expect("PgPool not configured")
        .clone();
    (config, pool)
}

/// Authenticate a bearer token: a session JWT, or an API key whose scope
/// allows `required_scope`. The user must be live either way.
fn authenticate(
    request: &HttpRequest,
    required_scope: ApiKeyScope,
) -> Pin<Box<dyn Future<Output = Result<AuthUser>>>> {
    let (config, pool) = config_and_pool(request);
    let sso_state = request.app_data::<SharedSsoState>().cloned();
    let rate_limits = request.app_data::<Data<RateLimits>>().cloned();
    let failed_auth = FailedAuthLimit::new(request);
    let token = bearer_token(request);
    Box::pin(async move {
        // No token is not a failed guess, so it is not charged.
        let token = token.ok_or(Error::Unauthorized)?;
        let result = async {
            let (user_id, api_key_id) = if token.starts_with(auth::API_KEY_TOKEN_PREFIX) {
                // A JWT is checked without a query, an API key is not.
                failed_auth.check_blocked()?;
                let api_key =
                    api_keys::find_active(pool.get_ref(), &auth::hash_api_key_token(&token))
                        .await?
                        .ok_or(Error::Unauthorized)?;
                if !api_key.scope.allows(required_scope) {
                    return Err(Error::Forbidden);
                }
                (api_key.user_id, Some(api_key.id))
            } else {
                let claims = auth::decode_jwt(&token, &config.auth.jwt_secret)?;
                (claims.user_id()?, None)
            };
            // Before the user lookup, so a flood stops costing queries.
            if let Some(rate_limits) = &rate_limits {
                rate_limits.check_user(user_id)?;
            }
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
            users::update_last_seen(pool.get_ref(), row.id, api_key_id).await?;
            Ok(AuthUser {
                user_id: row.id,
                is_admin: row.is_admin && api_key_id.is_none(),
                api_key_id,
            })
        }
        .await;
        failed_auth.charge(result)
    })
}

impl FromRequest for AuthUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self>>>>;

    fn from_request(request: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        authenticate(request, ApiKeyScope::Full)
    }
}

/// Upload caller: a session user or API key of any scope (see [`AuthUser`]), or the SSO provider
/// uploading on behalf of one of its users. The provider authenticates with
/// the SSO client credentials (`Authorization: Basic`) and names the user in
/// `On-Behalf-Of-Sub` (plus `On-Behalf-Of-Username` for first provisioning).
/// The user maps to the same shadow row an SSO login would. Only the upload
/// routes take this extractor, so the client secret opens nothing else.
pub struct UploadUser {
    pub user_id: Uuid,
    /// The API key the upload authenticated with, recorded on the file.
    pub api_key_id: Option<Uuid>,
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

    fn from_request(request: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let Some((client_id, client_secret)) = basic_credentials(request) else {
            let authenticated = authenticate(request, ApiKeyScope::Upload);
            return Box::pin(async move {
                let authenticated = authenticated.await?;
                Ok(UploadUser {
                    user_id: authenticated.user_id,
                    api_key_id: authenticated.api_key_id,
                })
            });
        };
        let (config, pool) = config_and_pool(request);
        let rate_limits = request.app_data::<Data<RateLimits>>().cloned();
        let failed_auth = FailedAuthLimit::new(request);
        let subject = header_value(request, "On-Behalf-Of-Sub");
        let username = header_value(request, "On-Behalf-Of-Username").unwrap_or_default();
        Box::pin(async move {
            failed_auth.check_blocked()?;
            let sso = failed_auth.charge(config.auth.sso.as_ref().ok_or(Error::Unauthorized))?;
            if !(client_id == sso.client_id && secrets_match(&client_secret, &sso.client_secret)) {
                return failed_auth.charge(Err(Error::Unauthorized));
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
                false,
            )
            .await?;
            if let Some(rate_limits) = &rate_limits {
                rate_limits.check_user(row.id)?;
            }
            if row.banned {
                return Err(Error::Unauthorized);
            }
            users::update_last_seen(pool.get_ref(), row.id, None).await?;
            Ok(UploadUser {
                user_id: row.id,
                api_key_id: None,
            })
        })
    }
}
