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

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub upload_dir: String,
    #[serde(default)]
    pub max_file_size_bytes: u64,
    #[serde(default)]
    pub max_width_pixels: u32,
    #[serde(default)]
    pub max_height_pixels: u32,
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
        config::Config::builder()
            .add_source(config::File::from(path.as_path()))
            .build()
            .expect("failed to read config.yml")
            .try_deserialize()
            .expect("invalid config.yml")
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
