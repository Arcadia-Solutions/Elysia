use elysia::config::{AuthConfig, Config, DatabaseConfig, ServerConfig, StorageConfig};

fn test_config_with_secret(secret: &str) -> Config {
    Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 8080,
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
        },
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
