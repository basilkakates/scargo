use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::api::privacy;
use crate::config::Settings;
use crate::db::{migrate, Database};
use crate::ingest::{bulk_ingest_reader, vin, vin_to_uuid, BulkMetricCache};

#[derive(Debug, Clone)]
pub struct Args {
    pub ingest_path: PathBuf,
    pub rebuild_db: bool,
    pub api_token: Option<String>,
    pub user_key: Option<String>,
    pub workers: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Summary {
    pub files_seen: usize,
    pub files_ingested: usize,
    pub files_duplicate: usize,
    pub files_failed: usize,
    pub rows_ingested: usize,
}

#[derive(Debug)]
pub struct FileFailure {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Default)]
pub struct RunResult {
    pub summary: Summary,
    pub failures: Vec<FileFailure>,
}

#[derive(Debug)]
struct VehicleResult {
    summary: Summary,
    failures: Vec<FileFailure>,
}

#[derive(Debug, Clone, Copy)]
enum FileStatus {
    Ingested(usize),
    Duplicate,
}

pub fn parse_args(argv: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = argv.into_iter();
    let _bin = args.next();
    let mut ingest_path = None;
    let mut rebuild_db = false;
    let mut api_token = non_empty_env("SCARGO_API_TOKEN");
    let mut user_key = non_empty_env("SCARGO_USER_KEY");
    let mut workers = 4usize;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rebuild-db" => rebuild_db = true,
            "--api-token" => {
                api_token = Some(
                    args.next()
                        .ok_or_else(|| "--api-token requires a value".to_string())?,
                );
            }
            "--user-key" => {
                user_key = Some(
                    args.next()
                        .ok_or_else(|| "--user-key requires a value".to_string())?,
                );
            }
            "--workers" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "--workers requires a value".to_string())?;
                workers = raw
                    .parse::<usize>()
                    .map_err(|_| "--workers must be a positive integer".to_string())?;
            }
            _ if arg.starts_with('-') => {
                return Err(format!("unknown argument: {arg}"));
            }
            _ => {
                if ingest_path.is_some() {
                    return Err("only one ingest path may be provided".into());
                }
                ingest_path = Some(PathBuf::from(arg));
            }
        }
    }

    Ok(Args {
        ingest_path: ingest_path.ok_or_else(|| {
            "usage: scargo-bulk-ingest <drop-root> [--rebuild-db] [--api-token TOKEN] [--workers N]"
                .to_string()
        })?,
        rebuild_db,
        api_token,
        user_key,
        workers: workers.max(1),
    })
}

pub async fn run(args: Args) -> Result<RunResult, String> {
    let settings = Settings::read().map_err(|e| format!("configuration error: {e}"))?;
    let db = Database::connect(&settings.database_url)
        .await
        .map_err(|e| format!("database connect failed: {e}"))?;

    if args.rebuild_db {
        migrate::rebuild_for_bulk_load(&db)
            .await
            .map_err(|e| format!("bulk rebuild failed: {e}"))?;
    } else {
        migrate::run(&db)
            .await
            .map_err(|e| format!("database bootstrap failed: {e}"))?;
    }

    let grouped = collect_vehicle_files(&args.ingest_path)?;
    let account_id = bulk_account_id(&db, &args).await?;
    let semaphore = Arc::new(Semaphore::new(args.workers));
    let mut tasks = Vec::with_capacity(grouped.len());

    for (vin, files) in grouped {
        let db = db.clone();
        let permit = semaphore.clone();
        tasks.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await.expect("semaphore open");
            process_vehicle(db, account_id, vin, files).await
        }));
    }

    let mut result = RunResult::default();
    for task in tasks {
        let vehicle = task
            .await
            .map_err(|e| format!("bulk worker join failed: {e}"))??;
        result.summary.files_seen += vehicle.summary.files_seen;
        result.summary.files_ingested += vehicle.summary.files_ingested;
        result.summary.files_duplicate += vehicle.summary.files_duplicate;
        result.summary.files_failed += vehicle.summary.files_failed;
        result.summary.rows_ingested += vehicle.summary.rows_ingested;
        result.failures.extend(vehicle.failures);
    }

    migrate::finalize_bulk_load(&db)
        .await
        .map_err(|e| format!("bulk finalize failed: {e}"))?;

    Ok(result)
}

async fn process_vehicle(
    db: Database,
    account_id: uuid::Uuid,
    vin: String,
    files: Vec<PathBuf>,
) -> Result<VehicleResult, String> {
    let vehicle_id = vin_to_uuid(&vin);
    let mut cache = BulkMetricCache::default();
    let mut summary = Summary::default();
    let mut failures = Vec::new();

    for path in files {
        summary.files_seen += 1;
        match process_file(&db, account_id, vehicle_id, &vin, &path, &mut cache).await {
            Ok(FileStatus::Ingested(rows)) => {
                summary.files_ingested += 1;
                summary.rows_ingested += rows;
            }
            Ok(FileStatus::Duplicate) => {
                summary.files_duplicate += 1;
            }
            Err(error) => {
                summary.files_failed += 1;
                failures.push(FileFailure { path, error });
            }
        }
    }

    Ok(VehicleResult { summary, failures })
}

async fn bulk_account_id(db: &Database, args: &Args) -> Result<uuid::Uuid, String> {
    let client = db
        .get()
        .await
        .map_err(|e| format!("pool checkout failed: {e}"))?;
    if let Some(token) = args.api_token.as_deref() {
        return privacy::api_token_account_id(&client, token)
            .await
            .map_err(|e| format!("api token lookup failed: {e}"))?
            .ok_or_else(|| "invalid API token".to_string());
    }
    if let Some(user_key) = args.user_key.as_deref() {
        let account_id = privacy::account_id_from_user_key(Some(user_key));
        privacy::ensure_account(&client, account_id)
            .await
            .map_err(|e| format!("ensure account failed: {e}"))?;
        return Ok(account_id);
    }
    Ok(privacy::ensure_guest_account(&client)
        .await
        .map_err(|e| format!("ensure guest account failed: {e}"))?
        .id)
}

async fn process_file(
    db: &Database,
    account_id: uuid::Uuid,
    vehicle_id: uuid::Uuid,
    vin: &str,
    path: &Path,
    cache: &mut BulkMetricCache,
) -> Result<FileStatus, String> {
    let body = fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    let digest = packet_hash(&body);
    let metadata = vin::decode(vin);
    let client = db
        .get()
        .await
        .map_err(|e| format!("pool checkout failed: {e}"))?;
    privacy::ensure_account(&client, account_id)
        .await
        .map_err(|e| format!("ensure account failed: {e}"))?;
    client
        .execute(
            "INSERT INTO vehicle (id, vin, make, model, year)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
                make = CASE
                    WHEN EXCLUDED.make <> '' THEN EXCLUDED.make
                    ELSE vehicle.make
                END,
                model = CASE
                    WHEN EXCLUDED.model <> '' THEN EXCLUDED.model
                    ELSE vehicle.model
                END,
                year = CASE
                    WHEN EXCLUDED.year > 0 THEN EXCLUDED.year
                    ELSE vehicle.year
                END,
                updated_at = NOW()",
            &[
                &vehicle_id,
                &vin,
                &metadata.make,
                &metadata.model,
                &metadata.year,
            ],
        )
        .await
        .map_err(|_| "ensure vehicle failed".to_string())?;
    let upload_id = uuid::Uuid::new_v4();
    let upload = client
        .query_one(
            "WITH inserted AS (
                INSERT INTO ingest_upload
                    (id, vehicle_id, content_hash, content_type, bytes)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (vehicle_id, content_hash) DO NOTHING
                RETURNING id, TRUE AS inserted
             )
             SELECT id, inserted FROM inserted
             UNION ALL
             SELECT id, FALSE AS inserted
             FROM ingest_upload
             WHERE vehicle_id = $2 AND content_hash = $3
               AND NOT EXISTS (SELECT 1 FROM inserted)",
            &[
                &upload_id,
                &vehicle_id,
                &digest,
                &"text/csv",
                &(body.len() as i64),
            ],
        )
        .await
        .map_err(|_| "reserve ingest_upload failed".to_string())?;
    let upload_id: uuid::Uuid = upload.get(0);
    let inserted: bool = upload.get(1);
    privacy::link_upload_to_account(&client, account_id, upload_id, vehicle_id)
        .await
        .map_err(|e| format!("link upload failed: {e}"))?;
    drop(client);

    if !inserted {
        return Ok(FileStatus::Duplicate);
    }

    let rows = match bulk_ingest_reader(body.as_slice(), vin, upload_id, db, cache).await {
        Ok(rows) => rows,
        Err(error) => {
            let cleanup = db
                .get()
                .await
                .map_err(|e| format!("pool checkout failed during cleanup: {e}"))?;
            let _ = cleanup
                .execute(
                    "DELETE FROM ingest_upload
                     WHERE id = $1",
                    &[&upload_id],
                )
                .await;
            return Err(format!("ingest failed for {}: {error}", path.display()));
        }
    };

    let update = db
        .get()
        .await
        .map_err(|e| format!("pool checkout failed for finalize: {e}"))?;
    update
        .execute(
            "UPDATE ingest_upload
             SET rows_ingested = $2, completed_at = NOW()
             WHERE id = $1",
            &[&upload_id, &(rows as i64)],
        )
        .await
        .map_err(|_| "update ingest_upload failed".to_string())?;
    Ok(FileStatus::Ingested(rows))
}

fn collect_vehicle_files(root: &Path) -> Result<BTreeMap<String, Vec<PathBuf>>, String> {
    let mut grouped = BTreeMap::<String, Vec<PathBuf>>::new();
    visit_dir(root, root, &mut grouped)?;
    for files in grouped.values_mut() {
        files.sort();
    }
    Ok(grouped)
}

fn visit_dir(
    root: &Path,
    dir: &Path,
    grouped: &mut BTreeMap<String, Vec<PathBuf>>,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("read_dir {} failed: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("dir entry {} failed: {e}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("file type {} failed: {e}", path.display()))?;
        if file_type.is_dir() {
            visit_dir(root, &path, grouped)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("csv") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| format!("relative path {} failed: {e}", path.display()))?;
        if rel
            .components()
            .any(|component| component.as_os_str() == ".processed")
        {
            continue;
        }
        if rel.components().count() < 2 {
            continue;
        }
        let Some(vehicle) = rel.components().next() else {
            continue;
        };
        grouped
            .entry(vehicle.as_os_str().to_string_lossy().into_owned())
            .or_default()
            .push(path);
    }
    Ok(())
}

fn packet_hash(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("string write");
    }
    out
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

    #[test]
    fn parse_args_requires_path() {
        let err = parse_args(["scargo-bulk-ingest".to_string()]).unwrap_err();
        assert!(err.contains("usage: scargo-bulk-ingest"));
    }

    #[test]
    fn parse_args_supports_rebuild_and_workers() {
        let args = parse_args(
            [
                "scargo-bulk-ingest",
                "drop-root",
                "--rebuild-db",
                "--api-token",
                "scargo_test",
                "--workers",
                "8",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert!(args.rebuild_db);
        assert_eq!(args.api_token.as_deref(), Some("scargo_test"));
        assert_eq!(args.workers, 8);
        assert_eq!(args.ingest_path, PathBuf::from("drop-root"));
    }
}
