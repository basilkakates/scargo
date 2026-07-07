use crate::api::{dropbox, ingest};
use crate::config::DropboxConfig;
use crate::db::Database;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio_postgres::types::ToSql;
use uuid::Uuid;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

const LIST_FOLDER_URL: &str = "https://api.dropboxapi.com/2/files/list_folder";
const LIST_FOLDER_CONTINUE_URL: &str = "https://api.dropboxapi.com/2/files/list_folder/continue";
const DOWNLOAD_URL: &str = "https://content.dropboxapi.com/2/files/download";
const TOKEN_URL: &str = "https://api.dropboxapi.com/oauth2/token";

pub fn spawn(db: Database, cfg: DropboxConfig) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let api = ReqwestDropboxApi::new();
        let sleep = Duration::from_secs(cfg.poll_sec.max(1));
        loop {
            if let Err(err) = sync_once(&db, &cfg, &api).await {
                tracing::warn!("dropbox worker pass failed: {err:?}");
            }
            tokio::time::sleep(sleep).await;
        }
    })
}

pub async fn sync_once(
    db: &Database,
    cfg: &DropboxConfig,
    api: &impl DropboxApi,
) -> Result<(), Error> {
    for connection in active_connections(db).await? {
        if let Err(err) = sync_connection(db, cfg, api, &connection).await {
            mark_connection_error(db, connection.id, "dropbox sync failed").await?;
            tracing::warn!(connection_id = %connection.id, "dropbox sync failed: {err:?}");
        }
    }
    Ok(())
}

pub async fn sync_account_once(
    db: &Database,
    cfg: &DropboxConfig,
    account_id: Uuid,
) -> Result<(), Error> {
    let Some(connection) = account_connection(db, account_id).await? else {
        return Err(Error::NotFound("dropbox connection".into()));
    };
    let api = ReqwestDropboxApi::new();
    if let Err(err) = sync_connection(db, cfg, &api, &connection).await {
        mark_connection_error(db, connection.id, "dropbox sync failed").await?;
        tracing::warn!(connection_id = %connection.id, "dropbox sync failed: {err:?}");
        return Err(err);
    }
    Ok(())
}

pub trait DropboxApi: Send + Sync {
    fn refresh_access_token<'a>(
        &'a self,
        cfg: &'a DropboxConfig,
        refresh_token: &'a str,
    ) -> BoxFuture<'a, Result<String, Error>>;

    fn list_folder<'a>(
        &'a self,
        access_token: &'a str,
        root_path: &'a str,
    ) -> BoxFuture<'a, Result<ListResult, Error>>;

    fn list_folder_continue<'a>(
        &'a self,
        access_token: &'a str,
        cursor: &'a str,
    ) -> BoxFuture<'a, Result<ListResult, Error>>;

    fn download<'a>(
        &'a self,
        access_token: &'a str,
        path_lower: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, Error>>;
}

#[derive(Clone)]
struct Connection {
    id: Uuid,
    account_id: Uuid,
    root_path: String,
    cursor: Option<String>,
    encrypted_refresh_token: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub id: Option<String>,
    pub path_lower: String,
    pub rev: String,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropboxEntry {
    File(FileEntry),
    Deleted {
        id: Option<String>,
        path_lower: String,
    },
}

#[derive(Debug, Clone)]
pub struct ListResult {
    pub entries: Vec<DropboxEntry>,
    pub cursor: String,
    pub has_more: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum PathAction {
    Ingest { vin: String },
    Skip { error: &'static str },
    Ignore,
}

async fn sync_connection(
    db: &Database,
    cfg: &DropboxConfig,
    api: &impl DropboxApi,
    connection: &Connection,
) -> Result<(), Error> {
    let refresh_token = dropbox::decrypt_token(
        &cfg.token_encryption_key,
        &connection.encrypted_refresh_token,
    )?;
    let access_token = api.refresh_access_token(cfg, &refresh_token).await?;
    let mut result = match connection.cursor.as_deref() {
        Some(cursor) => api.list_folder_continue(&access_token, cursor).await?,
        None => {
            api.list_folder(&access_token, &connection.root_path)
                .await?
        }
    };

    loop {
        for entry in &result.entries {
            apply_entry(db, api, connection, &access_token, entry).await?;
        }
        if !result.has_more {
            mark_connection_success(db, connection.id, &result.cursor).await?;
            return Ok(());
        }
        result = api
            .list_folder_continue(&access_token, &result.cursor)
            .await?;
    }
}

async fn apply_entry(
    db: &Database,
    api: &impl DropboxApi,
    connection: &Connection,
    access_token: &str,
    entry: &DropboxEntry,
) -> Result<(), Error> {
    match entry {
        DropboxEntry::Deleted { id, path_lower } => {
            record_deleted(db, connection, id.as_deref(), path_lower).await
        }
        DropboxEntry::File(file) => match map_path(&connection.root_path, &file.path_lower) {
            PathAction::Ingest { vin } => {
                if already_done(db, connection.id, &file.path_lower, &file.rev).await? {
                    return Ok(());
                }
                let body = api.download(access_token, &file.path_lower).await?;
                let result = ingest::ingest_csv_for_account(
                    db,
                    connection.account_id,
                    &vin,
                    &body,
                    "dropbox",
                )
                .await?;
                record_file(
                    db,
                    connection,
                    file,
                    Some(&vin),
                    if result.duplicate {
                        "duplicate"
                    } else {
                        "ingested"
                    },
                    result.rows_ingested,
                    result.duplicate,
                    Some(result.upload_id),
                    Some(result.content_hash.as_str()),
                    None,
                )
                .await
            }
            PathAction::Skip { error } => {
                record_file(
                    db,
                    connection,
                    file,
                    None,
                    "skipped",
                    0,
                    false,
                    None,
                    file.content_hash.as_deref(),
                    Some(error),
                )
                .await
            }
            PathAction::Ignore => Ok(()),
        },
    }?;
    Ok(())
}

fn map_path(root_path: &str, path_lower: &str) -> PathAction {
    let root = root_path.trim_matches('/').to_ascii_lowercase();
    let path = path_lower.trim_matches('/');
    let Some(rest) = path.strip_prefix(&root) else {
        return PathAction::Ignore;
    };
    let rest = rest.trim_start_matches('/');
    if rest.is_empty() {
        return PathAction::Ignore;
    }
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [file] if file.ends_with(".csv") => PathAction::Skip {
            error: "CSV is directly under Dropbox root; put it in a VIN folder",
        },
        [_vin, file] if file.ends_with(".csv") => PathAction::Ingest {
            vin: parts[0].to_string(),
        },
        [.., file] if file.ends_with(".csv") => PathAction::Skip {
            error: "Nested Dropbox CSV paths are not supported in v1",
        },
        _ => PathAction::Ignore,
    }
}

async fn active_connections(db: &Database) -> Result<Vec<Connection>, Error> {
    let client = db.get().await?;
    let rows = client
        .query(
            "SELECT id, account_id, root_path, cursor, encrypted_refresh_token
             FROM dropbox_connection
             WHERE status = 'active'",
            &[],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(rows
        .into_iter()
        .map(|row| Connection {
            id: row.get(0),
            account_id: row.get(1),
            root_path: row.get(2),
            cursor: row.get(3),
            encrypted_refresh_token: row.get(4),
        })
        .collect())
}

async fn account_connection(db: &Database, account_id: Uuid) -> Result<Option<Connection>, Error> {
    let client = db.get().await?;
    let row = client
        .query_opt(
            "SELECT id, account_id, root_path, cursor, encrypted_refresh_token
             FROM dropbox_connection
             WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row.map(|row| Connection {
        id: row.get(0),
        account_id: row.get(1),
        root_path: row.get(2),
        cursor: row.get(3),
        encrypted_refresh_token: row.get(4),
    }))
}

async fn already_done(
    db: &Database,
    connection_id: Uuid,
    path_lower: &str,
    rev: &str,
) -> Result<bool, Error> {
    let client = db.get().await?;
    let row = client
        .query_one(
            "SELECT EXISTS (
                SELECT 1 FROM dropbox_ingest_file
                WHERE connection_id = $1
                  AND path_lower = $2
                  AND COALESCE(rev, '') = $3
                  AND status IN ('ingested', 'duplicate')
             )",
            &[&connection_id, &path_lower, &rev],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row.get(0))
}

async fn record_file(
    db: &Database,
    connection: &Connection,
    file: &FileEntry,
    vin: Option<&str>,
    status: &str,
    rows_ingested: i64,
    duplicate: bool,
    upload_id: Option<Uuid>,
    content_hash: Option<&str>,
    latest_error: Option<&str>,
) -> Result<(), Error> {
    let client = db.get().await?;
    let id = Uuid::new_v4();
    let params: &[&(dyn ToSql + Sync)] = &[
        &id,
        &connection.id,
        &connection.account_id,
        &file.id,
        &file.path_lower,
        &file.rev,
        &content_hash,
        &vin,
        &upload_id,
        &status,
        &rows_ingested,
        &duplicate,
        &latest_error,
    ];
    client
        .execute(
            "INSERT INTO dropbox_ingest_file
                (id, connection_id, account_id, dropbox_file_id, path_lower, rev,
                 content_hash, vin, upload_id, status, rows_ingested, duplicate,
                 latest_error, seen_at, ingested_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                     NOW(), CASE WHEN $10 IN ('ingested', 'duplicate') THEN NOW() ELSE NULL END)
             ON CONFLICT (connection_id, path_lower, COALESCE(rev, ''))
             DO UPDATE SET
                dropbox_file_id = EXCLUDED.dropbox_file_id,
                content_hash = EXCLUDED.content_hash,
                vin = EXCLUDED.vin,
                upload_id = EXCLUDED.upload_id,
                status = EXCLUDED.status,
                rows_ingested = EXCLUDED.rows_ingested,
                duplicate = EXCLUDED.duplicate,
                latest_error = EXCLUDED.latest_error,
                seen_at = NOW(),
                ingested_at = EXCLUDED.ingested_at",
            params,
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

async fn record_deleted(
    db: &Database,
    connection: &Connection,
    dropbox_file_id: Option<&str>,
    path_lower: &str,
) -> Result<(), Error> {
    let client = db.get().await?;
    let id = Uuid::new_v4();
    client
        .execute(
            "INSERT INTO dropbox_ingest_file
                (id, connection_id, account_id, dropbox_file_id, path_lower, status, seen_at)
             VALUES ($1, $2, $3, $4, $5, 'deleted', NOW())
             ON CONFLICT (connection_id, path_lower, COALESCE(rev, ''))
             DO UPDATE SET status = 'deleted', seen_at = NOW(), latest_error = NULL",
            &[
                &id,
                &connection.id,
                &connection.account_id,
                &dropbox_file_id,
                &path_lower,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

async fn mark_connection_success(
    db: &Database,
    connection_id: Uuid,
    cursor: &str,
) -> Result<(), Error> {
    let client = db.get().await?;
    client
        .execute(
            "UPDATE dropbox_connection
             SET cursor = $2,
                 status = 'active',
                 latest_error = (
                    SELECT latest_error
                    FROM dropbox_ingest_file
                    WHERE connection_id = $1
                      AND latest_error IS NOT NULL
                    ORDER BY seen_at DESC
                    LIMIT 1
                 ),
                 last_sync_at = NOW(),
                 last_success_at = NOW(),
                 updated_at = NOW()
             WHERE id = $1",
            &[&connection_id, &cursor],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

async fn mark_connection_error(
    db: &Database,
    connection_id: Uuid,
    latest_error: &str,
) -> Result<(), Error> {
    let client = db.get().await?;
    client
        .execute(
            "UPDATE dropbox_connection
                 SET status = 'active',
                 latest_error = $2,
                 last_sync_at = NOW(),
                 updated_at = NOW()
             WHERE id = $1",
            &[&connection_id, &latest_error],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

struct ReqwestDropboxApi {
    client: reqwest::Client,
}

impl ReqwestDropboxApi {
    fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl DropboxApi for ReqwestDropboxApi {
    fn refresh_access_token<'a>(
        &'a self,
        cfg: &'a DropboxConfig,
        refresh_token: &'a str,
    ) -> BoxFuture<'a, Result<String, Error>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct TokenResponse {
                access_token: String,
            }

            let response = self
                .client
                .post(TOKEN_URL)
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh_token),
                    ("client_id", cfg.app_key.as_str()),
                    ("client_secret", cfg.app_secret.as_str()),
                ])
                .send()
                .await
                .map_err(|_| Error::Database)?;
            if !response.status().is_success() {
                return Err(Error::BadRequest("dropbox token refresh failed".into()));
            }
            response
                .json::<TokenResponse>()
                .await
                .map(|value| value.access_token)
                .map_err(|_| Error::Database)
        })
    }

    fn list_folder<'a>(
        &'a self,
        access_token: &'a str,
        root_path: &'a str,
    ) -> BoxFuture<'a, Result<ListResult, Error>> {
        Box::pin(async move {
            #[derive(Serialize)]
            struct Body<'a> {
                path: &'a str,
                recursive: bool,
            }

            let response = self
                .client
                .post(LIST_FOLDER_URL)
                .bearer_auth(access_token)
                .json(&Body {
                    path: root_path,
                    recursive: true,
                })
                .send()
                .await
                .map_err(|_| Error::Database)?;
            parse_list_response(response).await
        })
    }

    fn list_folder_continue<'a>(
        &'a self,
        access_token: &'a str,
        cursor: &'a str,
    ) -> BoxFuture<'a, Result<ListResult, Error>> {
        Box::pin(async move {
            #[derive(Serialize)]
            struct Body<'a> {
                cursor: &'a str,
            }

            let response = self
                .client
                .post(LIST_FOLDER_CONTINUE_URL)
                .bearer_auth(access_token)
                .json(&Body { cursor })
                .send()
                .await
                .map_err(|_| Error::Database)?;
            parse_list_response(response).await
        })
    }

    fn download<'a>(
        &'a self,
        access_token: &'a str,
        path_lower: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, Error>> {
        Box::pin(async move {
            #[derive(Serialize)]
            struct Arg<'a> {
                path: &'a str,
            }

            let arg =
                serde_json::to_string(&Arg { path: path_lower }).map_err(|_| Error::Internal)?;
            let response = self
                .client
                .post(DOWNLOAD_URL)
                .bearer_auth(access_token)
                .header("Dropbox-API-Arg", arg)
                .send()
                .await
                .map_err(|_| Error::Database)?;
            if !response.status().is_success() {
                return Err(Error::BadRequest("dropbox file download failed".into()));
            }
            response
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(|_| Error::Database)
        })
    }
}

async fn parse_list_response(response: reqwest::Response) -> Result<ListResult, Error> {
    if !response.status().is_success() {
        return Err(Error::BadRequest("dropbox list folder failed".into()));
    }
    response
        .json::<ApiListResult>()
        .await
        .map(Into::into)
        .map_err(|_| Error::Database)
}

#[derive(Deserialize)]
struct ApiListResult {
    entries: Vec<ApiEntry>,
    cursor: String,
    has_more: bool,
}

#[derive(Deserialize)]
#[serde(tag = ".tag")]
enum ApiEntry {
    #[serde(rename = "file")]
    File {
        id: Option<String>,
        path_lower: String,
        rev: String,
        content_hash: Option<String>,
    },
    #[serde(rename = "deleted")]
    Deleted {
        id: Option<String>,
        path_lower: String,
    },
    #[serde(other)]
    Other,
}

impl From<ApiListResult> for ListResult {
    fn from(value: ApiListResult) -> Self {
        Self {
            entries: value.entries.into_iter().filter_map(Into::into).collect(),
            cursor: value.cursor,
            has_more: value.has_more,
        }
    }
}

impl From<ApiEntry> for Option<DropboxEntry> {
    fn from(value: ApiEntry) -> Self {
        match value {
            ApiEntry::File {
                id,
                path_lower,
                rev,
                content_hash,
            } => Some(DropboxEntry::File(FileEntry {
                id,
                path_lower,
                rev,
                content_hash,
            })),
            ApiEntry::Deleted { id, path_lower } => Some(DropboxEntry::Deleted { id, path_lower }),
            ApiEntry::Other => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_direct_vin_csv() {
        assert_eq!(
            map_path(
                "/OBD Fusion/CsvLogs",
                "/obd fusion/csvlogs/1hgcm82633a004352/trip.csv"
            ),
            PathAction::Ingest {
                vin: "1hgcm82633a004352".into()
            }
        );
    }

    #[test]
    fn root_csv_is_visible_skip() {
        assert_eq!(
            map_path("/OBD Fusion/CsvLogs", "/obd fusion/csvlogs/trip.csv"),
            PathAction::Skip {
                error: "CSV is directly under Dropbox root; put it in a VIN folder"
            }
        );
    }

    #[test]
    fn nested_csv_is_visible_skip() {
        assert_eq!(
            map_path(
                "/OBD Fusion/CsvLogs",
                "/obd fusion/csvlogs/VIN/nested/trip.csv"
            ),
            PathAction::Skip {
                error: "Nested Dropbox CSV paths are not supported in v1"
            }
        );
    }

    #[test]
    fn non_csv_is_ignored() {
        assert_eq!(
            map_path("/OBD Fusion/CsvLogs", "/obd fusion/csvlogs/VIN/readme.txt"),
            PathAction::Ignore
        );
    }
}
