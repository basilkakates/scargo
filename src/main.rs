// ── crate root ──────────────────────────────────────────────
// scargo: OBD2 telematics ingestion, analysis, dashboard.
// See AGENTS.md for architecture, conventions, and workflow.
// ────────────────────────────────────────────────────────────

use actix_files as fs;
use actix_web::{web, App, HttpServer};
use scargo::api;
use scargo::config::Settings;
use scargo::db::{self, Database};
use std::io;

const MAX_CSV_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = dotenvy::dotenv();

    // Init tracing.  Respect RUST_LOG; fall back to info.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let settings = Settings::read().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("configuration error: {e}"),
        )
    })?;

    tracing::info!(
        scargo_env = %settings.env,
        database_url_source = settings.database_url_source,
        "Database configuration resolved"
    );
    let db = Database::connect(&settings.database_url)
        .await
        .expect("Failed to connect to database");

    db::migrate::run(&db)
        .await
        .expect("Failed to run database migrations");
    if let Some(cfg) = settings.dropbox.clone() {
        scargo::dropbox_worker::spawn(db.clone(), cfg);
    }

    let bind = format!("{}:{}", settings.http.host, settings.http.port);
    tracing::info!("Starting HTTP server on {bind}");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db.clone()))
            .app_data(web::Data::new(settings.clone()))
            .app_data(web::PayloadConfig::new(MAX_CSV_UPLOAD_BYTES))
            .configure(api::routes::configure)
            .service(fs::Files::new("/static", "dashboard/static"))
            .service(fs::Files::new("/", "dashboard/static").index_file("index.html"))
    })
    .bind(&bind)?
    .run()
    .await
}
