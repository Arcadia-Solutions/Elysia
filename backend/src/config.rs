use serde::Deserialize;

use crate::services::image::TargetFormat;

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

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub upload_dir: String,
    #[serde(default)]
    pub max_file_size_bytes: u64,
    #[serde(default)]
    pub max_width_pixels: u32,
    #[serde(default)]
    pub max_height_pixels: u32,
    #[serde(default)]
    pub target_width_pixels: u32,
    #[serde(default)]
    pub target_height_pixels: u32,
    #[serde(default)]
    pub target_file_format: Option<TargetFormat>,
    #[serde(default)]
    pub target_file_size_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    pub admin_token: String,
}

impl Config {
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
        config.validate().expect("invalid config.yml");
        config
    }

    /// Reject contradictory storage settings: processing options require a
    /// target_file_format, since we will not re-encode to an unknown format.
    pub fn validate(&self) -> Result<(), String> {
        let s = &self.storage;
        if s.target_file_format.is_none()
            && (s.target_width_pixels > 0
                || s.target_height_pixels > 0
                || s.target_file_size_bytes > 0)
        {
            return Err(
                "storage.target_file_format is required when target_width_pixels, \
                 target_height_pixels or target_file_size_bytes is set"
                    .into(),
            );
        }
        // An upload with no byte ceiling is an unbounded disk/memory write, so
        // require one always.
        if s.max_file_size_bytes == 0 {
            return Err("storage.max_file_size_bytes must be > 0".into());
        }
        // Processing additionally decodes the source into memory
        // (width * height * 4 bytes), so it must run behind pixel caps or a small
        // file declaring huge dimensions is a decompression bomb.
        if s.target_file_format.is_some() && (s.max_width_pixels == 0 || s.max_height_pixels == 0) {
            return Err(
                "storage.max_width_pixels and max_height_pixels must be > 0 when \
                 target_file_format is set (they bound the server-side decode)"
                    .into(),
            );
        }
        Ok(())
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
