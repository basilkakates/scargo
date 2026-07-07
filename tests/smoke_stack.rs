use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_postgres::{Config, NoTls};

const HTTP_HOST: &str = "127.0.0.1";
const DEFAULT_DB_PORT: &str = "5432";
const DEFAULT_HTTP_PORT: &str = "18080";
const DEFAULT_ADMIN_DB: &str = "postgres";
static SMOKE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
#[ignore = "requires a running Postgres/TimescaleDB service"]
fn smoke_existing_database_connects_ingests_and_reads() {
    let _guard = smoke_lock().lock().expect("smoke lock");
    let _ = dotenvy::from_filename(".env");
    let _ = dotenvy::from_filename(".env.smoke");
    let db = SmokeDatabase::create("existing");
    let mut app = App::start(&db);
    wait_for_health(&mut app);

    assert_contains(&http_get("/api/health", ""), "\"ok\"");
    let (cookie, token) = register_smoke_user("existing");
    http_get("/api/channels", &cookie_header(&cookie));

    let vin = format!("DEMO-HONDA-ACCORD-{}", std::process::id());
    let csv = "\
# StartTime = 03/27/2026 06:54:01.3973 PM
Time (sec),Engine RPM (RPM),Vehicle speed (MPH)
0.0,800,0
1.0,900,10
";
    assert_contains(
        &http_post(
            &format!("/api/ingest/csv?vin={vin}"),
            csv,
            &bearer_header(&token),
        ),
        "\"rows_ingested\":",
    );
    assert_contains(
        &http_get("/api/vehicles", &cookie_header(&cookie)),
        "\"reading_count\":",
    );
    assert_contains(
        &http_get("/api/channels", &cookie_header(&cookie)),
        "engine_rpm",
    );
    assert_contains(
        &http_get(
            "/api/analysis/dashboard?channels=engine_rpm&limit=10",
            &cookie_header(&cookie),
        ),
        "engine_rpm",
    );
}

#[test]
#[ignore = "requires a running Postgres/TimescaleDB service"]
fn smoke_bulk_rebuild_ingests_and_serves_summary() {
    let _guard = smoke_lock().lock().expect("smoke lock");
    let _ = dotenvy::from_filename(".env");
    let _ = dotenvy::from_filename(".env.smoke");
    let db = SmokeDatabase::create("bulk");
    let drop_root = TempDropRoot::create();
    std::fs::create_dir_all(drop_root.path().join("DEMO-HONDA-ACCORD")).expect("vehicle dir");
    std::fs::write(
        drop_root.path().join("DEMO-HONDA-ACCORD/good.csv"),
        "\
# StartTime = 03/27/2026 06:54:01.3973 PM
Time (sec),Engine RPM (RPM),Vehicle speed (MPH)
0.0,800,0
1.0,900,10
",
    )
    .expect("good csv");
    std::fs::write(
        drop_root.path().join("DEMO-HONDA-ACCORD/bad.csv"),
        "not,a,valid,obd,csv\n",
    )
    .expect("bad csv");

    let status = Command::new(env!("CARGO_BIN_EXE_scargo-bulk-ingest"))
        .arg(drop_root.path().as_os_str())
        .arg("--rebuild-db")
        .env_remove("SCARGO_DATABASE_URL")
        .env("SCARGO_ENV", "dev")
        .env("POSTGRES_HOST", env_or("POSTGRES_HOST", "127.0.0.1"))
        .env("POSTGRES_PORT", env_or("POSTGRES_PORT", DEFAULT_DB_PORT))
        .env("POSTGRES_USER", env_or("POSTGRES_USER", "scargo"))
        .env("POSTGRES_DB", db.name())
        .status()
        .expect("run bulk ingest");
    assert!(!status.success(), "expected non-zero exit for bad.csv");

    let mut app = App::start(&db);
    wait_for_health(&mut app);
    assert_contains(
        &http_get("/api/analysis/summary/engine_rpm?bucket=1d&limit=10", ""),
        "\"count\":2",
    );
}

struct App {
    child: Child,
}

impl App {
    fn start(db: &SmokeDatabase) -> Self {
        let http_port = http_port().to_string();
        let mut command = Command::new(env!("CARGO_BIN_EXE_scargo"));
        command
            .env_remove("SCARGO_DATABASE_URL")
            .env("SCARGO_ENV", "dev")
            .env("POSTGRES_HOST", env_or("POSTGRES_HOST", "127.0.0.1"))
            .env("POSTGRES_PORT", env_or("POSTGRES_PORT", DEFAULT_DB_PORT))
            .env("POSTGRES_USER", env_or("POSTGRES_USER", "scargo"))
            .env("POSTGRES_DB", db.name())
            .env("SCARGO_HTTP_HOST", HTTP_HOST)
            .env("SCARGO_HTTP_PORT", http_port)
            .env("RUST_LOG", "warn");
        if let Ok(password) = std::env::var("POSTGRES_PASSWORD") {
            command.env("POSTGRES_PASSWORD", password);
        }
        let child = command.spawn().expect("start scargo");
        Self { child }
    }
}

struct SmokeDatabase {
    name: String,
}

impl SmokeDatabase {
    fn create(suffix: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_millis();
        let name = format!("scargo_smoke_{}_{}_{}", std::process::id(), suffix, stamp);
        let db = Self { name };
        db.recreate();
        db
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn recreate(&self) {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            let client = admin_client().await;
            client
                .batch_execute(&format!(
                    "DROP DATABASE IF EXISTS {} WITH (FORCE);",
                    quoted_identifier(&self.name)
                ))
                .await
                .expect("drop stale smoke database");
            client
                .batch_execute(&format!(
                    "CREATE DATABASE {};",
                    quoted_identifier(&self.name)
                ))
                .await
                .expect("create smoke database");
        });
    }

    fn drop_database(&self) {
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return;
        };
        rt.block_on(async {
            let client = admin_client().await;
            let _ = client
                .batch_execute(&format!(
                    "DROP DATABASE IF EXISTS {} WITH (FORCE);",
                    quoted_identifier(&self.name)
                ))
                .await;
        });
    }
}

impl Drop for SmokeDatabase {
    fn drop(&mut self) {
        self.drop_database();
    }
}

async fn admin_client() -> tokio_postgres::Client {
    let (client, connection) = pg_config(admin_database())
        .connect(NoTls)
        .await
        .expect("connect admin db");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

fn pg_config(database: String) -> Config {
    let mut config = Config::new();
    config
        .host(env_or("POSTGRES_HOST", "127.0.0.1"))
        .port(
            env_or("POSTGRES_PORT", DEFAULT_DB_PORT)
                .parse()
                .expect("valid postgres port"),
        )
        .user(env_or("POSTGRES_USER", "scargo"))
        .dbname(&database);
    if let Ok(password) = std::env::var("POSTGRES_PASSWORD") {
        config.password(password);
    }
    config
}

fn admin_database() -> String {
    env_or("SCARGO_SMOKE_ADMIN_DB", DEFAULT_ADMIN_DB)
}

fn quoted_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_health(app: &mut App) {
    for _ in 0..60 {
        if try_http_get("/api/health", "")
            .map(|response| response.contains("\"ok\""))
            .unwrap_or(false)
        {
            return;
        }
        if let Some(status) = app.child.try_wait().expect("app status") {
            panic!("scargo exited before health check: {status}");
        }
        thread::sleep(Duration::from_secs(1));
    }
    panic!("timed out waiting for scargo health");
}

fn register_smoke_user(suffix: &str) -> (String, String) {
    let username = format!("smoke_{suffix}_{}", std::process::id());
    let body = format!(r#"{{"username":"{username}","password":"smoke-password"}}"#);
    let response = http_post(
        "/api/auth/register",
        &body,
        "Content-Type: application/json\r\n",
    );
    let cookie = extract_cookie(&response);
    let payload: serde_json::Value =
        serde_json::from_str(response_body(&response)).expect("register json");
    let token = payload["upload_token"]
        .as_str()
        .expect("upload token")
        .to_string();
    (cookie, token)
}

fn http_get(path: &str, auth_header: &str) -> String {
    try_http("GET", path, "", auth_header).expect("http get")
}

fn http_post(path: &str, body: &str, auth_header: &str) -> String {
    try_http("POST", path, body, auth_header).expect("http post")
}

fn try_http_get(path: &str, auth_header: &str) -> std::io::Result<String> {
    try_http("GET", path, "", auth_header)
}

fn try_http(method: &str, path: &str, body: &str, auth_header: &str) -> std::io::Result<String> {
    let mut stream = TcpStream::connect((HTTP_HOST, http_port()))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {HTTP_HOST}\r\n{auth_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    ?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    assert_contains(&response, "HTTP/1.1 200");
    Ok(response)
}

fn cookie_header(cookie: &str) -> String {
    format!("Cookie: {cookie}\r\n")
}

fn bearer_header(token: &str) -> String {
    format!("Authorization: Bearer {token}\r\nContent-Type: text/csv\r\n")
}

fn extract_cookie(response: &str) -> String {
    response
        .lines()
        .find_map(|line| {
            line.strip_prefix("set-cookie: ")
                .or_else(|| line.strip_prefix("Set-Cookie: "))
        })
        .and_then(|value| value.split(';').next())
        .expect("set-cookie header")
        .to_string()
}

fn response_body(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
}

fn http_port() -> u16 {
    env_or("SCARGO_SMOKE_HTTP_PORT", DEFAULT_HTTP_PORT)
        .parse()
        .expect("valid smoke HTTP port")
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "missing {needle:?} in response:\n{haystack}"
    );
}

struct TempDropRoot {
    path: PathBuf,
}

impl TempDropRoot {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "scargo-bulk-smoke-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_millis()
        ));
        if path.exists() {
            let _ = std::fs::remove_dir_all(&path);
        }
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDropRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn smoke_lock() -> &'static Mutex<()> {
    SMOKE_LOCK.get_or_init(|| Mutex::new(()))
}
