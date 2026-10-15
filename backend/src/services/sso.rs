use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use actix_web::web::Data;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand_core::{OsRng, RngCore};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::config::SsoConfig;
use crate::error::{Error, Result};

/// Shared SSO client state: one HTTP client, the discovered provider
/// endpoints and the ban status cache. Register as app data alongside the
/// pool and config.
pub type SharedSsoState = Data<SsoState>;

/// OpenID Connect scopes every login requests.
const SSO_SCOPE: &str = "openid profile email";
/// How long a ban status verdict is trusted before re-checking the provider.
const STATUS_TTL: Duration = Duration::from_secs(60);

/// HTTP client, provider endpoints (discovered on first use, retried until
/// the provider answers) and the short-lived ban status cache, keyed by
/// local user id. The cache holds at most one entry per SSO user.
pub struct SsoState {
    client: reqwest::Client,
    endpoints: OnceCell<Endpoints>,
    status: Mutex<HashMap<Uuid, StatusEntry>>,
}

/// The parts of the provider's discovery document the login flow uses.
#[derive(Debug, Deserialize)]
struct Endpoints {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
}

struct StatusEntry {
    active: bool,
    checked_at: Instant,
}

impl Default for SsoState {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("failed to build SSO HTTP client"),
            endpoints: OnceCell::new(),
            status: Mutex::new(HashMap::new()),
        }
    }
}

impl SsoState {
    /// The provider endpoints from `{issuer}/.well-known/openid-configuration`.
    /// The document must name the configured issuer, so a misconfigured or
    /// spoofed provider is refused.
    async fn endpoints(&self, config: &SsoConfig) -> Result<&Endpoints> {
        self.endpoints
            .get_or_try_init(|| async {
                let issuer = config.normalized_issuer();
                let endpoints: Endpoints = self
                    .client
                    .get(format!("{issuer}/.well-known/openid-configuration"))
                    .send()
                    .await
                    .and_then(|response| response.error_for_status())
                    .map_err(log_as("discovery", Error::Internal))?
                    .json()
                    .await
                    .map_err(log_as("discovery document", Error::Internal))?;
                if endpoints.issuer.trim_end_matches('/') != issuer {
                    return Err(log_as("discovery", Error::Internal)(format!(
                        "the document names issuer {}, not the configured {issuer}",
                        endpoints.issuer
                    )));
                }
                Ok(endpoints)
            })
            .await
    }
}

/// Logs why a provider call failed, then answers `answer`: the cause is only
/// useful to whoever configures SSO, never to the caller.
fn log_as<E: std::fmt::Display, T>(context: &'static str, answer: T) -> impl FnOnce(E) -> T {
    move |error| {
        tracing::warn!("sso {context} failed: {error}");
        answer
    }
}

/// 16 random bytes, base64url-encoded: the `state` CSRF token.
pub fn generate_state() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes::<16>())
}

/// 32 random bytes, base64url-encoded: the PKCE code verifier.
pub fn generate_verifier() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes::<32>())
}

fn random_bytes<const LENGTH: usize>() -> [u8; LENGTH] {
    let mut bytes = [0u8; LENGTH];
    OsRng
        .try_fill_bytes(&mut bytes)
        .expect("system RNG failure");
    bytes
}

/// S256 PKCE challenge for a verifier: base64url of its SHA-256.
pub fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The provider authorize URL the browser must visit to start a login.
pub async fn authorization_url(
    sso_state: &SharedSsoState,
    config: &SsoConfig,
    state: &str,
    challenge: &str,
) -> Result<String> {
    let endpoints = sso_state.endpoints(config).await?;
    let mut url =
        url::Url::parse(&endpoints.authorization_endpoint).map_err(|_| Error::Internal)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("scope", SSO_SCOPE)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.as_str().to_string())
}

/// The provider's token endpoint response: an access token for userinfo.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// The provider's userinfo response: who just signed in.
#[derive(Debug, Deserialize)]
pub struct Identity {
    pub sub: String,
    #[serde(default)]
    pub preferred_username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// Exchange the callback code for an access token, then fetch who signed in
/// from userinfo. Both calls go straight to the provider over TLS, so the
/// answer is trusted without validating the ID token.
pub async fn fetch_identity(
    state: &SharedSsoState,
    config: &SsoConfig,
    code: &str,
    verifier: &str,
) -> Result<Identity> {
    let endpoints = state.endpoints(config).await?;
    let token: TokenResponse = state
        .client
        .post(&endpoints.token_endpoint)
        .basic_auth(&config.client_id, Some(&config.client_secret))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(log_as("token request", Error::Internal))?
        .error_for_status()
        .map_err(log_as("token request", Error::Unauthorized))?
        .json()
        .await
        .map_err(log_as(
            "token response",
            Error::BadRequest("malformed token response".into()),
        ))?;
    let identity: Identity = state
        .client
        .get(&endpoints.userinfo_endpoint)
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(log_as("userinfo request", Error::Internal))?
        .error_for_status()
        .map_err(log_as("userinfo request", Error::Unauthorized))?
        .json()
        .await
        .map_err(log_as(
            "userinfo response",
            Error::BadRequest("malformed userinfo response".into()),
        ))?;
    if identity.sub.is_empty() {
        return Err(Error::Unauthorized);
    }
    Ok(identity)
}

/// The status endpoint's response: false when banned or unknown.
#[derive(Debug, Deserialize)]
struct StatusResponse {
    active: bool,
}

/// Whether the provider still lets the user behind an SSO row in, per the
/// optional status endpoint (always true without one). Answers are cached 60
/// seconds per user. Transport failures, refusals and malformed answers count
/// as inactive (fail closed) but are not cached, so a provider outage ends
/// with the outage.
pub async fn user_active(
    state: &SharedSsoState,
    config: &SsoConfig,
    user_id: Uuid,
    subject: &str,
) -> bool {
    let Some(status_endpoint) = config.status_endpoint.as_deref() else {
        return true;
    };
    if let Some(entry) = state
        .status
        .lock()
        .expect("status lock poisoned")
        .get(&user_id)
        && entry.checked_at.elapsed() < STATUS_TTL
    {
        return entry.active;
    }
    let response = state
        .client
        // form encoding turns spaces into `+`, which a path reads literally
        .get(
            status_endpoint.replace(
                "{sub}",
                &url::form_urlencoded::byte_serialize(subject.as_bytes())
                    .collect::<String>()
                    .replace('+', "%20"),
            ),
        )
        .basic_auth(&config.client_id, Some(&config.client_secret))
        .send()
        .await
        .and_then(|response| response.error_for_status());
    let response = match response {
        Ok(response) => response,
        Err(error) => return log_as("status request", false)(error),
    };
    let status = match response.json::<StatusResponse>().await {
        Ok(status) => status,
        Err(error) => return log_as("status response", false)(error),
    };
    state.status.lock().expect("status lock poisoned").insert(
        user_id,
        StatusEntry {
            active: status.active,
            checked_at: Instant::now(),
        },
    );
    status.active
}
