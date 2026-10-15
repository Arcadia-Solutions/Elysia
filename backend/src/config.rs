use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
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
    /// Hard multipart stream ceiling, baked into the server at startup. Bounds
    /// the bytes streamed to the temp file; the UI-editable
    /// `max_file_size_bytes` enforces the user-facing cap per request.
    #[serde(default = "default_upload_stream_bytes")]
    pub max_upload_stream_bytes: u64,
}

fn default_upload_stream_bytes() -> u64 {
    10 * 1024 * 1024 * 1024
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
    /// Refuse to boot with a placeholder or short session secret.
    pub fn validate(&self) {
        if self.auth.jwt_secret.len() < MINIMUM_JWT_SECRET_LENGTH
            || self.auth.jwt_secret == EXAMPLE_JWT_SECRET
        {
            panic!(
                "auth.jwt_secret must be a random value of at least {MINIMUM_JWT_SECRET_LENGTH} characters, not the config.example.yml placeholder"
            );
        }
    }

    /// Loads `config.yml`. Honors `ELYSIA_CONFIG` (explicit path); otherwise walks
    /// up from the cwd until it finds one.
    pub fn load() -> Self {
        let path = Self::find_config();
        let config: Config = config::Config::builder()
            .add_source(config::File::from(path.as_path()))
            .build()
            .expect("failed to read config.yml")
            .try_deserialize()
            .expect("invalid config.yml");
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
