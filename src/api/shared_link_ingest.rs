use crate::config::Settings;
use crate::db::Database;
use crate::ingest::vin;
use crate::Error;
use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use tokio::time::{sleep, Duration};
use uuid::Uuid;
use zip::ZipArchive;

#[derive(Deserialize)]
pub struct SharedLinkInput {
    url: String,
}

#[derive(Serialize)]
pub struct SharedLinkStatus {
    configured: bool,
    active: bool,
    link_label: String,
    last_sync_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    latest_error: String,
    ingested_count: i64,
    duplicate_count: i64,
    skipped_count: i64,
}

#[get("/ingest-sources/shared-link")]
pub async fn get_source(db: web::Data<Database>, req: HttpRequest) -> Result<HttpResponse, Error> {
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::Unauthorized);
    }
    Ok(HttpResponse::Ok().json(status_for_account(&client, account.id).await?))
}

#[put("/ingest-sources/shared-link")]
pub async fn put_source(
    db: web::Data<Database>,
    settings: web::Data<Settings>,
    req: HttpRequest,
    input: web::Json<SharedLinkInput>,
) -> Result<HttpResponse, Error> {
    let url = normalize_dropbox_url(input.url.trim())?;
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::Unauthorized);
    }
    let source_id = Uuid::new_v4();
    let encrypted = encrypt(&url, &settings.shared_link_secret)?;
    let label = redacted_label(&url);
    let poll = settings.shared_link_poll_seconds as i32;
    client
        .execute(
            "INSERT INTO shared_ingest_source
                (id, account_id, encrypted_url, link_label, poll_interval_seconds, next_poll_at)
             VALUES ($1, $2, $3, $4, $5, NOW())
             ON CONFLICT (account_id) DO UPDATE SET
                encrypted_url = EXCLUDED.encrypted_url,
                link_label = EXCLUDED.link_label,
                status = 'active',
                poll_interval_seconds = EXCLUDED.poll_interval_seconds,
                next_poll_at = NOW(),
                latest_error = '',
                updated_at = NOW()",
            &[&source_id, &account.id, &encrypted, &label, &poll],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(HttpResponse::Ok().json(status_for_account(&client, account.id).await?))
}

#[delete("/ingest-sources/shared-link")]
pub async fn delete_source(
    db: web::Data<Database>,
    req: HttpRequest,
) -> Result<HttpResponse, Error> {
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::Unauthorized);
    }
    client
        .execute(
            "DELETE FROM shared_ingest_source WHERE account_id = $1",
            &[&account.id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(HttpResponse::Ok().json(status_for_account(&client, account.id).await?))
}

#[post("/ingest-sources/shared-link/pause")]
pub async fn pause_source(
    db: web::Data<Database>,
    req: HttpRequest,
) -> Result<HttpResponse, Error> {
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::Unauthorized);
    }
    client
        .execute(
            "UPDATE shared_ingest_source
             SET status = CASE WHEN status = 'active' THEN 'paused' ELSE 'active' END,
                 updated_at = NOW()
             WHERE account_id = $1",
            &[&account.id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(HttpResponse::Ok().json(status_for_account(&client, account.id).await?))
}

#[post("/ingest-sources/shared-link/sync-now")]
pub async fn sync_now(
    db: web::Data<Database>,
    settings: web::Data<Settings>,
    req: HttpRequest,
) -> Result<HttpResponse, Error> {
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::Unauthorized);
    }
    drop(client);
    sync_account(&db, &settings, account.id).await?;
    let client = db.get().await?;
    Ok(HttpResponse::Ok().json(status_for_account(&client, account.id).await?))
}

pub fn spawn_worker(db: Database, settings: Settings) {
    if !settings.shared_link_ingest {
        return;
    }
    tokio::spawn(async move {
        loop {
            if let Err(err) = sync_due_sources(&db, &settings).await {
                tracing::warn!("shared-link worker failed: {err}");
            }
            sleep(Duration::from_secs(
                settings.shared_link_poll_seconds.max(60),
            ))
            .await;
        }
    });
}

async fn sync_due_sources(db: &Database, settings: &Settings) -> Result<(), Error> {
    let client = db.get().await?;
    let rows = client
        .query(
            "SELECT account_id
             FROM shared_ingest_source
             WHERE status = 'active'
               AND (next_poll_at IS NULL OR next_poll_at <= NOW())",
            &[],
        )
        .await
        .map_err(|_| Error::Database)?;
    drop(client);
    for row in rows {
        sync_account(db, settings, row.get(0)).await?;
    }
    Ok(())
}

async fn sync_account(db: &Database, settings: &Settings, account_id: Uuid) -> Result<(), Error> {
    let client = db.get().await?;
    let row = client
        .query_opt(
            "SELECT id, encrypted_url, poll_interval_seconds
             FROM shared_ingest_source
             WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(|_| Error::Database)?
        .ok_or_else(|| Error::NotFound("shared link source".into()))?;
    let source_id: Uuid = row.get(0);
    let encrypted: String = row.get(1);
    let poll_seconds: i32 = row.get(2);
    let url = decrypt(&encrypted, &settings.shared_link_secret)?;
    drop(client);

    let result = sync_archive(db, account_id, source_id, &url).await;
    let client = db.get().await?;
    match result {
        Ok(()) => {
            client
                .execute(
                    "UPDATE shared_ingest_source
                     SET last_sync_at = NOW(),
                         last_success_at = NOW(),
                         latest_error = '',
                         next_poll_at = NOW() + ($2::int * INTERVAL '1 second'),
                         updated_at = NOW()
                     WHERE id = $1",
                    &[&source_id, &poll_seconds],
                )
                .await
                .map_err(|_| Error::Database)?;
            Ok(())
        }
        Err(err) => {
            let message = err.to_string();
            client
                .execute(
                    "UPDATE shared_ingest_source
                     SET last_sync_at = NOW(),
                         latest_error = $2,
                         next_poll_at = NOW() + ($3::int * INTERVAL '1 second'),
                         updated_at = NOW()
                     WHERE id = $1",
                    &[&source_id, &message, &poll_seconds],
                )
                .await
                .map_err(|_| Error::Database)?;
            Err(err)
        }
    }
}

async fn sync_archive(
    db: &Database,
    account_id: Uuid,
    source_id: Uuid,
    url: &str,
) -> Result<(), Error> {
    let bytes = reqwest::get(download_url(url)?)
        .await
        .map_err(|_| Error::BadRequest("shared link download failed".into()))?
        .bytes()
        .await
        .map_err(|_| Error::BadRequest("shared link download failed".into()))?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| Error::BadRequest("shared link did not download as a zip archive".into()))?;

    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|_| Error::BadRequest("bad zip entry".into()))?;
        if file.is_dir() {
            continue;
        }
        let path = file.name().to_string();
        let Some(entry) = classify_archive_path(&path) else {
            continue;
        };
        if let ArchiveEntry::Skipped { reason } = entry {
            entries.push((path, ArchiveEntry::Skipped { reason }, Vec::new()));
            continue;
        }
        let ArchiveEntry::Csv { vehicle_key } = entry else {
            continue;
        };
        let mut body = Vec::new();
        file.read_to_end(&mut body)
            .map_err(|_| Error::BadRequest("bad zip entry".into()))?;
        entries.push((path, ArchiveEntry::Csv { vehicle_key }, body));
    }
    drop(archive);

    for (path, entry, body) in entries {
        if let ArchiveEntry::Skipped { reason } = entry {
            record_file(
                db, source_id, account_id, &path, "", "", None, "skipped", 0, reason,
            )
            .await?;
            continue;
        }
        let ArchiveEntry::Csv { vehicle_key } = entry else {
            continue;
        };
        let hash = super::ingest::packet_hash(&body);
        maybe_fetch_vin_metadata(db, &vehicle_key).await?;
        apply_cached_vehicle_metadata(db, &vehicle_key).await?;
        if already_ingested(db, source_id, &path, &hash).await? {
            continue;
        }
        let result =
            super::ingest::ingest_csv_for_account(db, account_id, &vehicle_key, &body, "text/csv")
                .await?;
        apply_cached_vehicle_metadata(db, &vehicle_key).await?;
        record_file(
            db,
            source_id,
            account_id,
            &path,
            &vehicle_key,
            &hash,
            Some(result.upload_id),
            if result.duplicate {
                "duplicate"
            } else {
                "ingested"
            },
            result.rows_ingested,
            "",
        )
        .await?;
    }
    Ok(())
}

async fn status_for_account(
    client: &tokio_postgres::Client,
    account_id: Uuid,
) -> Result<SharedLinkStatus, Error> {
    let row = client
        .query_opt(
            "SELECT s.status,
                    s.link_label,
                    s.last_sync_at,
                    s.last_success_at,
                    s.latest_error,
                    COUNT(f.*) FILTER (WHERE f.status = 'ingested')::BIGINT,
                    COUNT(f.*) FILTER (WHERE f.status = 'duplicate')::BIGINT,
                    COUNT(f.*) FILTER (WHERE f.status = 'skipped')::BIGINT
             FROM shared_ingest_source s
             LEFT JOIN shared_ingest_file f ON f.source_id = s.id
             WHERE s.account_id = $1
             GROUP BY s.id",
            &[&account_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row
        .map(|row| SharedLinkStatus {
            configured: true,
            active: row.get::<_, String>(0) == "active",
            link_label: row.get(1),
            last_sync_at: row.get(2),
            last_success_at: row.get(3),
            latest_error: row.get(4),
            ingested_count: row.get(5),
            duplicate_count: row.get(6),
            skipped_count: row.get(7),
        })
        .unwrap_or(SharedLinkStatus {
            configured: false,
            active: false,
            link_label: String::new(),
            last_sync_at: None,
            last_success_at: None,
            latest_error: String::new(),
            ingested_count: 0,
            duplicate_count: 0,
            skipped_count: 0,
        }))
}

async fn already_ingested(
    db: &Database,
    source_id: Uuid,
    path: &str,
    hash: &str,
) -> Result<bool, Error> {
    let client = db.get().await?;
    let row = client
        .query_one(
            "SELECT EXISTS (
                SELECT 1 FROM shared_ingest_file
                WHERE source_id = $1
                  AND path = $2
                  AND content_hash = $3
                  AND status IN ('ingested', 'duplicate')
             )",
            &[&source_id, &path, &hash],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row.get(0))
}

async fn record_file(
    db: &Database,
    source_id: Uuid,
    account_id: Uuid,
    path: &str,
    vehicle_key: &str,
    hash: &str,
    upload_id: Option<Uuid>,
    status: &str,
    rows: i64,
    latest_error: &str,
) -> Result<(), Error> {
    let client = db.get().await?;
    client
        .execute(
            "INSERT INTO shared_ingest_file
                (id, source_id, account_id, path, vehicle_key, content_hash, upload_id, status, rows_ingested, latest_error, ingested_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, CASE WHEN $8 IN ('ingested', 'duplicate') THEN NOW() ELSE NULL END)
             ON CONFLICT (source_id, path, content_hash) DO UPDATE SET
                upload_id = EXCLUDED.upload_id,
                status = EXCLUDED.status,
                rows_ingested = EXCLUDED.rows_ingested,
                latest_error = EXCLUDED.latest_error,
                seen_at = NOW(),
                ingested_at = EXCLUDED.ingested_at",
            &[
                &Uuid::new_v4(),
                &source_id,
                &account_id,
                &path,
                &vehicle_key,
                &hash,
                &upload_id,
                &status,
                &rows,
                &latest_error,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

async fn maybe_fetch_vin_metadata(db: &Database, vehicle_key: &str) -> Result<(), Error> {
    let vin = vehicle_key.trim().to_ascii_uppercase();
    if !is_exact_vin(&vin) {
        return Ok(());
    }
    let client = db.get().await?;
    let cached = client
        .query_opt(
            "SELECT status, next_retry_at FROM vin_decode_cache
             WHERE vin = $1",
            &[&vin],
        )
        .await
        .map_err(|_| Error::Database)?;
    drop(client);

    if let Some(row) = cached.as_ref() {
        let status: String = row.get(0);
        if status == "ok" {
            return Ok(());
        }
    }

    if let Some(meta) = infer_vin_metadata(db, &vin).await? {
        persist_vin_metadata(db, &vin, &meta).await?;
        return Ok(());
    }

    if let Some(row) = cached {
        let next_retry_at: Option<DateTime<Utc>> = row.get(1);
        if next_retry_at.is_some_and(|value| value > Utc::now()) {
            return Ok(());
        }
    }

    let metadata = fetch_vpic(&vin).await;
    match metadata {
        Ok(meta) => {
            persist_vin_metadata(db, &vin, &meta).await?;
        }
        Err(err) => {
            let message = err.to_string();
            let client = db.get().await?;
            client
                .execute(
                    "INSERT INTO vin_decode_cache
                        (vin, status, next_retry_at, latest_error)
                     VALUES ($1, 'error', NOW() + INTERVAL '1 day', $2)
                     ON CONFLICT (vin) DO UPDATE SET
                        status = 'error',
                        next_retry_at = NOW() + INTERVAL '1 day',
                        latest_error = EXCLUDED.latest_error",
                    &[&vin, &message],
                )
                .await
                .map_err(|_| Error::Database)?;
        }
    }
    Ok(())
}

struct VpicMetadata {
    status: String,
    year: i32,
    make: String,
    model: String,
    engine_family: String,
    source: String,
    raw: String,
}

async fn fetch_vpic(vin: &str) -> Result<VpicMetadata, Error> {
    let url = format!(
        "https://vpic.nhtsa.dot.gov/api/vehicles/DecodeVinValuesExtended/{vin}?format=json"
    );
    let text = reqwest::get(url)
        .await
        .map_err(|_| Error::BadRequest("vPIC request failed".into()))?
        .text()
        .await
        .map_err(|_| Error::BadRequest("vPIC response was invalid".into()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| Error::BadRequest("vPIC response was invalid".into()))?;
    let row = value
        .get("Results")
        .and_then(|results| results.as_array())
        .and_then(|results| results.first())
        .ok_or_else(|| Error::BadRequest("vPIC response had no result".into()))?;
    let make = pick(row, "Make");
    let model = pick(row, "Model");
    let year = pick(row, "ModelYear").parse().unwrap_or(0);
    let engine_family = normalize_engine_family(row);
    let status = if make.is_empty() || model.is_empty() || engine_family.is_empty() {
        "incomplete"
    } else {
        "ok"
    };
    Ok(VpicMetadata {
        status: status.into(),
        year,
        make,
        model,
        engine_family,
        source: "vpic".into(),
        raw: value.to_string(),
    })
}

async fn infer_vin_metadata(db: &Database, vin_value: &str) -> Result<Option<VpicMetadata>, Error> {
    let year = vin::decode(vin_value).year;
    let Some(key) = pattern_key(vin_value, year) else {
        return Ok(None);
    };
    let client = db.get().await?;
    let cache_rows = client
        .query(
            "SELECT vin, year, make, model, engine_family
             FROM vin_decode_cache
             WHERE status = 'ok'
               AND year > 0
               AND make <> ''
               AND model <> ''
               AND engine_family <> ''
               AND char_length(vin) = 17",
            &[],
        )
        .await
        .map_err(|_| Error::Database)?;
    let vehicle_rows = client
        .query(
            "SELECT vin, year, make, model, engine_family
             FROM vehicle
             WHERE year > 0
               AND make <> ''
               AND model <> ''
               AND engine_family <> ''
               AND char_length(vin) = 17",
            &[],
        )
        .await
        .map_err(|_| Error::Database)?;
    drop(client);

    let mut matches = HashMap::new();
    for row in cache_rows.iter().chain(vehicle_rows.iter()) {
        let candidate_vin: String = row.get(0);
        let candidate_year: i32 = row.get(1);
        let candidate = InferenceCandidate {
            make: row.get(2),
            model: row.get(3),
            engine_family: row.get(4),
        };
        if pattern_key(&candidate_vin, candidate_year).as_deref() != Some(key.as_str()) {
            continue;
        }
        if !candidate.is_complete() {
            continue;
        }
        matches.entry(candidate.dedupe_key()).or_insert(candidate);
    }

    if matches.len() != 1 {
        return Ok(None);
    }

    let candidate = matches.into_values().next().unwrap();
    Ok(Some(VpicMetadata {
        status: "ok".into(),
        year,
        make: candidate.make,
        model: candidate.model,
        engine_family: candidate.engine_family,
        source: "inferred".into(),
        raw: json!({ "pattern_key": key, "source": "inferred" }).to_string(),
    }))
}

async fn persist_vin_metadata(db: &Database, vin: &str, meta: &VpicMetadata) -> Result<(), Error> {
    let client = db.get().await?;
    client
        .execute(
            "INSERT INTO vin_decode_cache
                (vin, status, year, make, model, engine_family, raw_response, source, fetched_at, latest_error)
             VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, NOW(), '')
             ON CONFLICT (vin) DO UPDATE SET
                status = EXCLUDED.status,
                year = EXCLUDED.year,
                make = EXCLUDED.make,
                model = EXCLUDED.model,
                engine_family = EXCLUDED.engine_family,
                raw_response = EXCLUDED.raw_response,
                source = EXCLUDED.source,
                fetched_at = NOW(),
                next_retry_at = NULL,
                latest_error = ''",
            &[
                &vin,
                &meta.status,
                &meta.year,
                &meta.make,
                &meta.model,
                &meta.engine_family,
                &meta.raw,
                &meta.source,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;
    drop(client);
    apply_vehicle_metadata(db, vin, meta).await
}

async fn apply_cached_vehicle_metadata(db: &Database, vehicle_key: &str) -> Result<(), Error> {
    let vin = vehicle_key.trim().to_ascii_uppercase();
    if !is_exact_vin(&vin) {
        return Ok(());
    }
    let client = db.get().await?;
    let cached = client
        .query_opt(
            "SELECT status, year, make, model, engine_family, source, raw_response::text
             FROM vin_decode_cache
             WHERE vin = $1 AND status = 'ok'",
            &[&vin],
        )
        .await
        .map_err(|_| Error::Database)?;
    drop(client);
    let Some(row) = cached else {
        return Ok(());
    };
    let meta = VpicMetadata {
        status: row.get(0),
        year: row.get(1),
        make: row.get(2),
        model: row.get(3),
        engine_family: row.get(4),
        source: row.get(5),
        raw: row.get(6),
    };
    apply_vehicle_metadata(db, &vin, &meta).await
}

async fn apply_vehicle_metadata(
    db: &Database,
    vin: &str,
    meta: &VpicMetadata,
) -> Result<(), Error> {
    let client = db.get().await?;
    let row = client
        .query_opt(
            "SELECT year, make, model, engine_family FROM vehicle WHERE vin = $1",
            &[&vin],
        )
        .await
        .map_err(|_| Error::Database)?;
    let Some(row) = row else {
        return Ok(());
    };
    let next_year = fill_year(row.get(0), meta.year);
    let next_make = fill_text(row.get::<_, String>(1), &meta.make);
    let next_model = fill_text(row.get::<_, String>(2), &meta.model);
    let next_engine_family = fill_text(row.get::<_, String>(3), &meta.engine_family);
    client
        .execute(
            "UPDATE vehicle
             SET year = $2,
                 make = $3,
                 model = $4,
                 engine_family = $5,
                 updated_at = NOW()
             WHERE vin = $1",
            &[
                &vin,
                &next_year,
                &next_make,
                &next_model,
                &next_engine_family,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

fn normalize_dropbox_url(input: &str) -> Result<String, Error> {
    let mut url = Url::parse(input).map_err(|_| Error::BadRequest("invalid shared link".into()))?;
    if url.scheme() != "https" {
        return Err(Error::BadRequest("shared link must use https".into()));
    }
    let host = url.host_str().unwrap_or("");
    if !matches!(
        host,
        "dropbox.com" | "www.dropbox.com" | "dl.dropboxusercontent.com"
    ) {
        return Err(Error::BadRequest(
            "shared link must be a Dropbox URL".into(),
        ));
    }
    if host != "dl.dropboxusercontent.com"
        && !(url.path().starts_with("/s/")
            || url.path().starts_with("/sh/")
            || url.path().starts_with("/scl/fo/"))
    {
        return Err(Error::BadRequest(
            "shared link must be a Dropbox shared folder URL".into(),
        ));
    }
    let pairs = url
        .query_pairs()
        .filter(|(key, _)| key != "dl")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    {
        let mut query = url.query_pairs_mut();
        query.clear();
        for (key, value) in pairs {
            query.append_pair(&key, &value);
        }
        query.append_pair("dl", "1");
    }
    Ok(url.to_string())
}

fn download_url(url: &str) -> Result<String, Error> {
    normalize_dropbox_url(url)
}

fn redacted_label(url: &str) -> String {
    let parsed = Url::parse(url).ok();
    let host = parsed
        .as_ref()
        .and_then(|url| url.host_str())
        .unwrap_or("Dropbox");
    let token = parsed
        .as_ref()
        .and_then(|url| url.path_segments())
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .next_back()
                .unwrap_or("")
        })
        .unwrap_or("");
    let suffix = token
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if suffix.is_empty() {
        host.into()
    } else {
        format!("{host} ...{suffix}")
    }
}

#[derive(PartialEq, Eq, Debug)]
enum ArchiveEntry {
    Csv { vehicle_key: String },
    Skipped { reason: &'static str },
}

fn classify_archive_path(path: &str) -> Option<ArchiveEntry> {
    let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
    let is_csv = parts
        .last()
        .map(|name| name.to_ascii_lowercase().ends_with(".csv"))
        .unwrap_or(false);
    if !is_csv {
        return None;
    }
    match parts.len() {
        1 => Some(ArchiveEntry::Skipped {
            reason: "missing vehicle folder",
        }),
        2 => Some(ArchiveEntry::Csv {
            vehicle_key: parts[0].to_string(),
        }),
        _ => Some(ArchiveEntry::Skipped {
            reason: "nested CSV skipped in v1",
        }),
    }
}

fn is_exact_vin(value: &str) -> bool {
    value.len() == 17 && value.chars().all(|ch| ch.is_ascii_alphanumeric())
}

fn pattern_key(vin: &str, year: i32) -> Option<String> {
    let normalized = vin.trim().to_ascii_uppercase();
    if !is_exact_vin(&normalized) || year <= 0 {
        return None;
    }
    Some(format!("{}:{year}", &normalized[..8]))
}

fn fill_text(current: String, incoming: &str) -> String {
    if current.trim().is_empty() && !incoming.trim().is_empty() {
        incoming.trim().to_string()
    } else {
        current
    }
}

fn fill_year(current: i32, incoming: i32) -> i32 {
    if current <= 0 && incoming > 0 {
        incoming
    } else {
        current
    }
}

#[derive(Clone, Debug)]
struct InferenceCandidate {
    make: String,
    model: String,
    engine_family: String,
}

impl InferenceCandidate {
    fn is_complete(&self) -> bool {
        !self.make.trim().is_empty()
            && !self.model.trim().is_empty()
            && !self.engine_family.trim().is_empty()
    }

    fn dedupe_key(&self) -> (String, String, String) {
        (
            self.make.trim().to_ascii_lowercase(),
            self.model.trim().to_ascii_lowercase(),
            self.engine_family.trim().to_ascii_lowercase(),
        )
    }
}

fn pick(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn normalize_engine_family(row: &Value) -> String {
    let displacement = format_displacement(&pick(row, "DisplacementL"));
    let cylinders = pick(row, "EngineCylinders");
    if displacement.is_empty() || cylinders.is_empty() {
        return String::new();
    }
    let layout = normalize_cylinder_layout(&pick(row, "EngineConfiguration"));
    let aspiration = match pick(row, "AspirationType").to_ascii_lowercase().as_str() {
        "" | "naturally aspirated" | "na" | "no" | "false" | "0" => "NA".into(),
        "turbo" | "turbocharged" | "yes" | "true" | "1" => "Turbo".into(),
        other => other.to_string(),
    };
    let cylinder_family = if layout.is_empty() {
        format!("{cylinders}cyl")
    } else {
        format!("{layout}{cylinders}")
    };
    format!("{displacement}L {cylinder_family} {aspiration}")
}

fn format_displacement(value: &str) -> String {
    value
        .trim()
        .parse::<f64>()
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|_| value.trim().to_string())
}

fn normalize_cylinder_layout(value: &str) -> String {
    let text = value.trim().to_ascii_lowercase();
    if text.contains("v-shaped") || text == "v" {
        "V".into()
    } else if text.contains("inline") || matches!(text.as_str(), "i" | "in-line") {
        "I".into()
    } else if text.contains("flat") || text.contains("boxer") || text == "h" {
        "H".into()
    } else if text == "w" || text.contains("w-shaped") {
        "W".into()
    } else {
        String::new()
    }
}

fn encrypt(input: &str, secret: &str) -> Result<String, Error> {
    let cipher = Aes256Gcm::new_from_slice(&Sha256::digest(secret.as_bytes()))
        .map_err(|_| Error::Internal)?;
    let nonce_bytes = Uuid::new_v4();
    let nonce = Nonce::from_slice(&nonce_bytes.as_bytes()[..12]);
    let ciphertext = cipher
        .encrypt(nonce, input.as_bytes())
        .map_err(|_| Error::Internal)?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ciphertext);
    Ok(hex_encode(&out))
}

fn decrypt(input: &str, secret: &str) -> Result<String, Error> {
    let bytes = hex_decode(input)?;
    if bytes.len() < 13 {
        return Err(Error::Internal);
    }
    let cipher = Aes256Gcm::new_from_slice(&Sha256::digest(secret.as_bytes()))
        .map_err(|_| Error::Internal)?;
    let nonce = Nonce::from_slice(&bytes[..12]);
    let plaintext = cipher
        .decrypt(nonce, &bytes[12..])
        .map_err(|_| Error::Internal)?;
    String::from_utf8(plaintext).map_err(|_| Error::Internal)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(input: &str) -> Result<Vec<u8>, Error> {
    if !input.len().is_multiple_of(2) {
        return Err(Error::Internal);
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    for i in (0..input.len()).step_by(2) {
        out.push(u8::from_str_radix(&input[i..i + 2], 16).map_err(|_| Error::Internal)?);
    }
    Ok(out)
}

#[cfg(test)]
fn old_xor_crypt(input: &str, secret: &str) -> String {
    let key = Sha256::digest(secret.as_bytes());
    input
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(i, byte)| format!("{:02x}", byte ^ key[i % key.len()]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_and_redacts_dropbox_url() {
        let url = normalize_dropbox_url("https://www.dropbox.com/sh/abcdef123456?dl=0").unwrap();
        assert_eq!(url, "https://www.dropbox.com/sh/abcdef123456?dl=1");
        assert_eq!(redacted_label(&url), "www.dropbox.com ...3456");
        let folder_url = normalize_dropbox_url(
            "https://www.dropbox.com/scl/fo/fgj5fyfxse9rhnu2hsmlc/AO7l11OpkSQzD7n26no2_4Y?rlkey=vp1fr2exybsjivqtp61b3iwd1&st=dgxlda06&dl=0"
        ).unwrap();
        assert!(folder_url.contains("rlkey=vp1fr2exybsjivqtp61b3iwd1"));
        assert!(folder_url.contains("st=dgxlda06"));
        assert!(folder_url.ends_with("dl=1"));
        assert_eq!(redacted_label(&folder_url), "www.dropbox.com ...2_4Y");
        assert!(normalize_dropbox_url("http://www.dropbox.com/sh/abc").is_err());
        assert!(normalize_dropbox_url("https://example.com/sh/abc").is_err());
    }

    #[test]
    fn builds_pattern_keys_for_exact_vins_only() {
        assert_eq!(
            pattern_key("1HGCP3F89BA032306", 2011).as_deref(),
            Some("1HGCP3F8:2011")
        );
        assert_eq!(pattern_key("DEMO-HONDA-ACCORD", 2011), None);
        assert_eq!(pattern_key("1HGCP3F89BA032306", 0), None);
    }

    #[test]
    fn fills_only_blank_vehicle_fields() {
        assert_eq!(fill_text(String::new(), "Honda"), "Honda");
        assert_eq!(fill_text("Existing".into(), "Honda"), "Existing");
        assert_eq!(fill_year(0, 2011), 2011);
        assert_eq!(fill_year(2020, 2011), 2020);
    }

    #[test]
    fn classifies_archive_paths() {
        assert_eq!(
            classify_archive_path("VIN123/file.csv"),
            Some(ArchiveEntry::Csv {
                vehicle_key: "VIN123".into()
            })
        );
        assert_eq!(
            classify_archive_path("root.csv"),
            Some(ArchiveEntry::Skipped {
                reason: "missing vehicle folder"
            })
        );
        assert_eq!(
            classify_archive_path("VIN123/nested/file.csv"),
            Some(ArchiveEntry::Skipped {
                reason: "nested CSV skipped in v1"
            })
        );
        assert_eq!(classify_archive_path("VIN123/readme.txt"), None);
    }

    #[test]
    fn normalizes_engine_family_without_guessing_layout() {
        let row = json!({"DisplacementL": "2.35", "EngineCylinders": "4"});
        assert_eq!(normalize_engine_family(&row), "2.4L 4cyl NA");
        let row = json!({
            "DisplacementL": "1.5",
            "EngineCylinders": "4",
            "EngineConfiguration": "Inline",
            "AspirationType": "Turbocharged"
        });
        assert_eq!(normalize_engine_family(&row), "1.5L I4 Turbo");
    }

    #[test]
    fn crypt_round_trips() {
        let text = "https://www.dropbox.com/sh/abcdef";
        let encrypted = encrypt(text, "secret").unwrap();
        assert_ne!(encrypted, text);
        assert_eq!(decrypt(&encrypted, "secret").unwrap(), text);
        assert!(decrypt(&old_xor_crypt(text, "secret"), "secret").is_err());
    }
}
