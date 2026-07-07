const DEFAULT_ENV: &str = "dev";
const DEFAULT_POSTGRES_HOST: &str = "127.0.0.1";
const DEFAULT_POSTGRES_PORT: &str = "5432";
const DEFAULT_POSTGRES_USER: &str = "scargo";
const DEFAULT_POSTGRES_DB: &str = "scargo";
const DEFAULT_HTTP_HOST: &str = "127.0.0.1";
const DEFAULT_HTTP_PORT: u16 = 8080;
const DEFAULT_DROPBOX_ROOT_PATH: &str = "/OBD Fusion/CsvLogs";
const DEFAULT_DROPBOX_POLL_SEC: u64 = 300;

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct DropboxConfig {
    pub app_key: String,
    pub app_secret: String,
    pub base_url: String,
    pub token_encryption_key: [u8; 32],
    pub poll_sec: u64,
    pub root_path: &'static str,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub http: HttpConfig,
    pub env: String,
    pub database_url: String,
    pub database_url_source: &'static str,
    pub dropbox: Option<DropboxConfig>,
}

impl Settings {
    pub fn read() -> Result<Self, String> {
        let env =
            normalize_env(&non_empty_env("SCARGO_ENV").unwrap_or_else(|| DEFAULT_ENV.into()))?;
        let (database_url, database_url_source) = resolve_database_url(&env)?;

        Ok(Self {
            http: HttpConfig {
                host: non_empty_env("SCARGO_HTTP_HOST").unwrap_or_else(|| DEFAULT_HTTP_HOST.into()),
                port: parse_http_port()?,
            },
            env,
            database_url,
            database_url_source,
            dropbox: read_dropbox_config()?,
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        let (database_url, database_url_source) =
            local_database_url().expect("default local database URL must be valid");

        Self {
            http: HttpConfig {
                host: DEFAULT_HTTP_HOST.into(),
                port: DEFAULT_HTTP_PORT,
            },
            env: DEFAULT_ENV.into(),
            database_url,
            database_url_source,
            dropbox: None,
        }
    }
}

fn normalize_env(env: &str) -> Result<String, String> {
    let env = env.trim().to_ascii_lowercase();
    match env.as_str() {
        "dev" | "production" => Ok(env),
        _ => Err("SCARGO_ENV must be either 'dev' or 'production'".into()),
    }
}

fn resolve_database_url(env: &str) -> Result<(String, &'static str), String> {
    if let Some(url) = non_empty_env("SCARGO_DATABASE_URL") {
        return Ok((url, "SCARGO_DATABASE_URL"));
    }

    if env == "production" {
        return Err("SCARGO_DATABASE_URL is required when SCARGO_ENV=production".into());
    }

    local_database_url()
}

fn local_database_url() -> Result<(String, &'static str), String> {
    let host = non_empty_env("POSTGRES_HOST").unwrap_or_else(|| DEFAULT_POSTGRES_HOST.into());
    let port = non_empty_env("POSTGRES_PORT").unwrap_or_else(|| DEFAULT_POSTGRES_PORT.into());
    let user = non_empty_env("POSTGRES_USER").unwrap_or_else(|| DEFAULT_POSTGRES_USER.into());
    let db = non_empty_env("POSTGRES_DB").unwrap_or_else(|| DEFAULT_POSTGRES_DB.into());
    let auth = non_empty_env("POSTGRES_PASSWORD")
        .map(|password| format!(":{password}"))
        .unwrap_or_default();

    Ok((
        format!("postgres://{user}{auth}@{host}:{port}/{db}"),
        "local default",
    ))
}

fn parse_http_port() -> Result<u16, String> {
    match non_empty_env("SCARGO_HTTP_PORT") {
        Some(port) => port
            .parse()
            .map_err(|_| "SCARGO_HTTP_PORT must be a valid TCP port".into()),
        None => Ok(DEFAULT_HTTP_PORT),
    }
}

fn read_dropbox_config() -> Result<Option<DropboxConfig>, String> {
    if !env_flag("SCARGO_DROPBOX_ENABLED") {
        return Ok(None);
    }

    Ok(Some(DropboxConfig {
        app_key: required_env("DROPBOX_APP_KEY")?,
        app_secret: required_env("DROPBOX_APP_SECRET")?,
        base_url: normalize_base_url(&required_env("SCARGO_BASE_URL")?)?,
        token_encryption_key: parse_encryption_key(&required_env("SCARGO_TOKEN_ENCRYPTION_KEY")?)?,
        poll_sec: parse_dropbox_poll_sec()?,
        root_path: DEFAULT_DROPBOX_ROOT_PATH,
    }))
}

fn parse_dropbox_poll_sec() -> Result<u64, String> {
    match non_empty_env("SCARGO_DROPBOX_POLL_SEC") {
        Some(value) => value
            .parse()
            .map_err(|_| "SCARGO_DROPBOX_POLL_SEC must be a positive integer".into()),
        None => Ok(DEFAULT_DROPBOX_POLL_SEC),
    }
}

fn normalize_base_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() || !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err("SCARGO_BASE_URL must start with http:// or https://".into());
    }
    Ok(trimmed.to_string())
}

fn parse_encryption_key(value: &str) -> Result<[u8; 32], String> {
    if let Ok(bytes) = decode_hex(value.trim()) {
        return bytes
            .try_into()
            .map_err(|_| "SCARGO_TOKEN_ENCRYPTION_KEY hex value must decode to 32 bytes".into());
    }

    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|_| {
            "SCARGO_TOKEN_ENCRYPTION_KEY must be 32 raw bytes encoded as hex or base64".to_string()
        })?;
    decoded
        .try_into()
        .map_err(|_| "SCARGO_TOKEN_ENCRYPTION_KEY base64 value must decode to 32 bytes".into())
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid hex".into());
    }
    value
        .as_bytes()
        .chunks(2)
        .map(|chunk| {
            std::str::from_utf8(chunk)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| "invalid hex".to_string())
        })
        .collect()
}

fn env_flag(name: &str) -> bool {
    matches!(
        non_empty_env(name)
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes")
    )
}

fn required_env(name: &str) -> Result<String, String> {
    non_empty_env(name)
        .ok_or_else(|| format!("{name} is required when SCARGO_DROPBOX_ENABLED=true"))
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|value| {
        let value = value.trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn default_settings_compile() {
        let _ = Settings::default();
    }

    #[test]
    fn production_without_database_url_fails() {
        let _guard = env_lock().lock().unwrap();
        without_env("SCARGO_DATABASE_URL", || {
            let err = resolve_database_url("production").unwrap_err();
            assert!(err.contains("SCARGO_DATABASE_URL is required"));
        });
    }

    #[test]
    fn dev_uses_explicit_database_url() {
        let _guard = env_lock().lock().unwrap();
        with_env(
            "SCARGO_DATABASE_URL",
            "postgres://example.invalid/db",
            || {
                let (url, source) = resolve_database_url("dev").unwrap();
                assert_eq!(url, "postgres://example.invalid/db");
                assert_eq!(source, "SCARGO_DATABASE_URL");
            },
        );
    }

    #[test]
    fn dev_generates_local_database_url() {
        let _guard = env_lock().lock().unwrap();
        without_env("SCARGO_DATABASE_URL", || {
            without_env("POSTGRES_PASSWORD", || {
                with_env("POSTGRES_HOST", "localhost", || {
                    with_env("POSTGRES_PORT", "6543", || {
                        with_env("POSTGRES_USER", "scargo", || {
                            with_env("POSTGRES_DB", "scargo_test", || {
                                let (url, source) = resolve_database_url("dev").unwrap();
                                assert_eq!(url, "postgres://scargo@localhost:6543/scargo_test");
                                assert_eq!(source, "local default");
                            });
                        });
                    });
                });
            });
        });
    }

    #[test]
    fn dev_generates_password_database_url() {
        let _guard = env_lock().lock().unwrap();
        without_env("SCARGO_DATABASE_URL", || {
            with_env("POSTGRES_HOST", "localhost", || {
                with_env("POSTGRES_PORT", "6543", || {
                    with_env("POSTGRES_USER", "scargo", || {
                        with_env("POSTGRES_PASSWORD", "secret", || {
                            with_env("POSTGRES_DB", "scargo_test", || {
                                let (url, source) = resolve_database_url("dev").unwrap();
                                let expected = [
                                    "postgres://",
                                    "scargo",
                                    ":",
                                    "secret",
                                    "@localhost:6543/scargo_test",
                                ]
                                .concat();
                                assert_eq!(url, expected);
                                assert_eq!(source, "local default");
                            });
                        });
                    });
                });
            });
        });
    }

    #[test]
    fn dropbox_disabled_by_default() {
        let _guard = env_lock().lock().unwrap();
        without_env("SCARGO_DROPBOX_ENABLED", || {
            assert!(read_dropbox_config().unwrap().is_none());
        });
    }

    #[test]
    fn dropbox_enabled_requires_required_values() {
        let _guard = env_lock().lock().unwrap();
        with_env("SCARGO_DROPBOX_ENABLED", "true", || {
            let err = read_dropbox_config().unwrap_err();
            assert!(err.contains("DROPBOX_APP_KEY"));
        });
    }

    #[test]
    fn parses_hex_encryption_key() {
        let bytes = parse_encryption_key(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .unwrap();
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes[31], 31);
    }

    #[test]
    fn rejects_short_encryption_key() {
        let err = parse_encryption_key("abcd").unwrap_err();
        assert!(err.contains("32 bytes"));
    }

    fn env_lock() -> &'static Mutex<()> {
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_env(name: &str, value: &str, test: impl FnOnce()) {
        let original = std::env::var_os(name);
        std::env::set_var(name, value);
        test();
        restore_env(name, original);
    }

    fn without_env(name: &str, test: impl FnOnce()) {
        let original = std::env::var_os(name);
        std::env::remove_var(name);
        test();
        restore_env(name, original);
    }

    fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            std::env::set_var(name, value);
        } else {
            std::env::remove_var(name);
        }
    }
}
