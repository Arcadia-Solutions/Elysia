use elysia::config::{
    AuthConfig, Config, DatabaseConfig, ServerConfig, SsoConfig, StorageConfig, TelemetryConfig,
};

fn test_config_with_secret(secret: &str) -> Config {
    Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 8080,
            public_url: "http://elysia.test".into(),
        },
        database: DatabaseConfig {
            host: "localhost".into(),
            port: 5432,
            user: "elysia".into(),
            password: "secret".into(),
            name: "elysia".into(),
        },
        storage: StorageConfig {
            upload_dir: "/tmp/elysia".into(),
            max_upload_stream_bytes: 10 * 1024 * 1024,
        },
        auth: AuthConfig {
            jwt_secret: secret.into(),
            sso: None,
        },
        telemetry: TelemetryConfig::default(),
    }
}

#[test]
fn accepts_a_long_random_secret() {
    test_config_with_secret("a-long-random-secret-with-32-plus-characters").validate();
}

#[test]
#[should_panic(expected = "auth.jwt_secret")]
fn rejects_the_example_placeholder() {
    test_config_with_secret("change-me-to-a-long-random-secret").validate();
}

#[test]
#[should_panic(expected = "auth.jwt_secret")]
fn rejects_a_short_secret() {
    test_config_with_secret("too-short").validate();
}

#[test]
#[should_panic(expected = "server.public_url")]
fn rejects_sso_with_relative_image_links() {
    let mut config = test_config_with_secret("a-long-random-secret-with-32-plus-characters");
    config.server.public_url = String::new();
    config.auth.sso = Some(SsoConfig {
        name: "Arcadia".into(),
        issuer: "http://arcadia.test/api/oauth".into(),
        client_id: "elysia".into(),
        client_secret: "secret".into(),
        redirect_uri: "http://elysia.test/auth/sso/callback".into(),
        status_endpoint: None,
    });
    config.validate();
}

#[test]
fn loads_the_config_file_with_environment_overrides() {
    let dir = std::env::temp_dir().join(format!("elysia-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.yml");
    std::fs::write(&path, include_str!("../../config.example.yml")).unwrap();
    // SAFETY: the only test in this binary that touches the environment.
    unsafe {
        std::env::set_var("ELYSIA_CONFIG", &path);
        std::env::set_var("ELYSIA_DATABASE__PORT", "5433");
        std::env::set_var("ELYSIA_SERVER__PUBLIC_URL", "https://images.example.com/");
    }

    let config = Config::load();
    assert_eq!(
        std::path::Path::new(&config.storage.upload_dir),
        dir.join("./data/uploads")
    );
    assert_eq!(config.database.port, 5433);
    assert_eq!(config.server.public_url, "https://images.example.com");
    let _ = std::fs::remove_dir_all(&dir);
}
