const DEFAULT_ENV: &str = "dev";
const DEFAULT_POSTGRES_HOST: &str = "127.0.0.1";
const DEFAULT_POSTGRES_PORT: &str = "5432";
const DEFAULT_POSTGRES_USER: &str = "scargo";
const DEFAULT_POSTGRES_DB: &str = "scargo";
const DEFAULT_HTTP_HOST: &str = "127.0.0.1";
const DEFAULT_HTTP_PORT: u16 = 8080;

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub http: HttpConfig,
    pub env: String,
    pub database_url: String,
    pub database_url_source: &'static str,
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
