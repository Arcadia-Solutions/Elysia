use ipnet::IpNet;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    /// Absent, nothing is rate limited.
    pub rate_limit: Option<RateLimitConfig>,
    /// Proxy every outbound HTTP request goes through, e.g.
    /// `http://user:pass@proxy:3128`. Absent, requests go out directly.
    pub outbound_proxy: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Where Elysia is reached from the outside, e.g. `https://images.example.com`.
    /// Prefixes every image and thumbnail URL the API returns. Empty (the
    /// default) keeps them relative, which only works on Elysia's own site, so
    /// it is required with SSO. Stored without a trailing slash.
    #[serde(default)]
    pub public_url: String,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub name: String,
}

impl DatabaseConfig {
    pub fn url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.name
        )
    }
}

/// Boot-time storage infrastructure. The other settings
/// live in [`crate::settings::ElysiaSettings`] (DB-backed, edited in the UI).
#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub upload_dir: String,
}

/// OpenTelemetry export, disabled (stdout logs only) while `otlp_endpoint` is unset.
#[derive(Debug, Default, Deserialize)]
pub struct TelemetryConfig {
    /// OTLP gRPC collector, e.g. `http://localhost:4317`.
    pub otlp_endpoint: Option<String>,
}

/// Per-minute request budgets, refilled continuously. 0 disables a limit.
#[derive(Debug, Deserialize)]
pub struct RateLimitConfig {
    /// Routes that need no login (images, thumbnails, login, registration,
    /// SSO, public settings) and failed authentications, keyed by client IP
    /// (IPv6 by /64).
    pub unauthenticated_requests_per_minute_per_ip: u32,
    /// Login, registration and password change attempts, keyed by client IP,
    /// on top of the unauthenticated or authenticated budget.
    pub login_attempts_per_minute_per_ip: u32,
    /// API routes that need a login, keyed by user, whatever the token or IP.
    pub authenticated_requests_per_minute_per_user: u32,
    /// Reverse proxies whose `X-Forwarded-For` is believed. The client IP is
    /// the rightmost entry not in these ranges.
    pub trusted_proxies: Vec<IpNet>,
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    /// HS256 secret used to sign user session tokens.
    pub jwt_secret: String,
    /// OpenID Connect sign-on, disabled when absent so standalone deploys
    /// keep working.
    pub sso: Option<SsoConfig>,
}

/// OpenID Connect client settings. `redirect_uri` must be registered
/// verbatim as a redirect URI at the provider.
#[derive(Debug, Deserialize)]
pub struct SsoConfig {
    /// Provider name shown on the login button, e.g. `Arcadia`.
    pub name: String,
    /// Provider issuer, e.g. `https://arcadia.example.com/api/oauth`. The
    /// endpoints are read from `{issuer}/.well-known/openid-configuration`.
    pub issuer: String,
    /// This client's id as registered at the provider.
    pub client_id: String,
    pub client_secret: String,
    /// Where the provider sends the browser after sign-in: the frontend
    /// callback page (e.g. `https://elysia.example.com/auth/sso/callback`),
    /// which exchanges `?code&state` at `/api/auth/sso/callback`.
    pub redirect_uri: String,
    /// Optional live ban check, not part of OpenID Connect: a URL with a
    /// `{sub}` placeholder answering `{"active": bool}` to the client's Basic
    /// credentials, e.g. Arcadia's
    /// `https://arcadia.example.com/api/oauth/users/{sub}/status`. When set,
    /// SSO sessions are refused as soon as it stops answering `active`.
    #[serde(default)]
    pub status_endpoint: Option<String>,
}

impl SsoConfig {
    /// Issuer base without a trailing slash, so endpoint paths and the stored
    /// external issuer stay identical however the value was written in config.
    pub fn normalized_issuer(&self) -> &str {
        self.issuer.trim_end_matches('/')
    }
}

/// Secrets that must never sign session tokens.
const EXAMPLE_JWT_SECRET: &str = "change-me-to-a-long-random-secret";

/// Minimum HS256 secret length in bytes. Short secrets let attackers brute
/// force session tokens offline.
const MINIMUM_JWT_SECRET_LENGTH: usize = 32;

impl Config {
    /// Refuse to boot with a placeholder or short session secret, or with SSO
    /// but relative image links, which break on the provider's site.
    pub fn validate(&self) {
        if let Some(proxy) = &self.outbound_proxy {
            reqwest::Proxy::all(proxy).expect("outbound_proxy must be a valid proxy url");
        }
        if self.auth.sso.is_some() && self.server.public_url.is_empty() {
            panic!(
                "server.public_url must be set when auth.sso is: the SSO provider stores the image links Elysia returns"
            );
        }
        if self.auth.jwt_secret.len() < MINIMUM_JWT_SECRET_LENGTH
            || self.auth.jwt_secret == EXAMPLE_JWT_SECRET
        {
            panic!(
                "auth.jwt_secret must be a random value of at least {MINIMUM_JWT_SECRET_LENGTH} characters, not the config.example.yml placeholder"
            );
        }
    }

    /// Loads `config.yml`. Honors `ELYSIA_CONFIG` (explicit path); otherwise walks
    /// up from the cwd until it finds one. `ELYSIA_<SECTION>__<KEY>` environment
    /// variables override it, e.g. `ELYSIA_DATABASE__PASSWORD`.
    pub fn load() -> Self {
        let path = Self::find_config();
        let mut config: Config = config::Config::builder()
            .add_source(config::File::from(path.as_path()))
            .add_source(
                config::Environment::with_prefix("ELYSIA")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()
            .expect("failed to read config.yml")
            .try_deserialize()
            .expect("invalid config.yml");
        // The config is found by walking up from the cwd, so a relative
        // upload_dir must follow the config file, not the cwd: otherwise
        // running from a subdirectory reads and writes a different folder.
        // An absolute upload_dir is kept as is by `join`.
        let config_dir = path.parent().expect("config path has a parent");
        config.server.public_url = config.server.public_url.trim_end_matches('/').to_string();
        config.storage.upload_dir = config_dir
            .join(&config.storage.upload_dir)
            .to_string_lossy()
            .into_owned();
        config
    }

    fn find_config() -> std::path::PathBuf {
        if let Ok(p) = std::env::var("ELYSIA_CONFIG") {
            return p.into();
        }
        let mut dir = std::env::current_dir().expect("no cwd");
        loop {
            let candidate = dir.join("config.yml");
            if candidate.is_file() {
                return candidate;
            }
            if !dir.pop() {
                panic!("config.yml not found in cwd or any parent (set ELYSIA_CONFIG to override)");
            }
        }
    }
}

static OUTBOUND_PROXY: std::sync::OnceLock<reqwest::Proxy> = std::sync::OnceLock::new();

/// Route every client built by [`http_client_builder`] through `proxy`. Call
/// once at startup, before any client is built.
pub fn set_outbound_proxy(proxy: &str) {
    let proxy = reqwest::Proxy::all(proxy).expect("outbound_proxy must be a valid proxy url");
    OUTBOUND_PROXY
        .set(proxy)
        .expect("outbound proxy already set");
}

/// The configured outbound proxy, if any.
pub fn outbound_proxy() -> Option<&'static reqwest::Proxy> {
    OUTBOUND_PROXY.get()
}

/// Client builder honoring the configured outbound proxy.
pub fn http_client_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    match outbound_proxy() {
        Some(proxy) => builder.proxy(proxy.clone()),
        None => builder,
    }
}
