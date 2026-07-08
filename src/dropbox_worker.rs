use crate::api::{dropbox, ingest};
use crate::config::DropboxConfig;
use crate::db::Database;
use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
const CURRENT_ACCOUNT_URL: &str = "https://api.dropboxapi.com/2/users/get_current_account";

pub fn spawn(db: Database, cfg: DropboxConfig) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let api = ReqwestDropboxApi::new();
        let sleep = Duration::from_secs(cfg.poll_sec.max(1));
        loop {
            if let Err(err) = sync_due(&db, &cfg, &api).await {
                tracing::warn!("dropbox worker pass failed: {err:?}");
            }
            tokio::time::sleep(sleep).await;
        }
    })
}

pub fn spawn_account_sync(db: Database, cfg: DropboxConfig, account_id: Uuid) {
    tokio::spawn(async move {
        if let Err(err) = sync_account_once(&db, &cfg, account_id).await {
            tracing::warn!(account_id = %account_id, "dropbox manual sync failed: {err:?}");
        }
    });
}

async fn sync_due(db: &Database, cfg: &DropboxConfig, api: &impl DropboxApi) -> Result<(), Error> {
    for account_id in due_accounts(db, cfg.poll_sec).await? {
        if let Err(err) = sync_account_with_api(db, cfg, api, account_id).await {
            tracing::warn!(account_id = %account_id, "dropbox worker sync failed: {err:?}");
        }
    }
    Ok(())
}

pub async fn sync_account_once(
    db: &Database,
    cfg: &DropboxConfig,
    account_id: Uuid,
) -> Result<(), Error> {
    let api = ReqwestDropboxApi::new();
    sync_account_with_api(db, cfg, &api, account_id).await
}

async fn sync_account_with_api(
    db: &Database,
    cfg: &DropboxConfig,
    api: &impl DropboxApi,
    account_id: Uuid,
) -> Result<(), Error> {
    let Some(connection) = claim_account_connection(db, account_id, cfg.poll_sec).await? else {
        return Ok(());
    };

    let result = sync_connection(db, cfg, api, &connection).await;
    match result {
        Ok(()) => mark_connection_success(db, connection.id).await,
        Err(err) => {
            mark_connection_error(db, connection.id, "dropbox sync failed").await?;
            Err(err)
        }
    }
}

pub trait DropboxApi: Send + Sync {
    fn refresh_access_token<'a>(
        &'a self,
        cfg: &'a DropboxConfig,
        refresh_token: &'a str,
    ) -> BoxFuture<'a, Result<String, Error>>;

    fn current_account<'a>(
        &'a self,
        access_token: &'a str,
    ) -> BoxFuture<'a, Result<CurrentAccount, Error>>;

    fn list_folder<'a>(
        &'a self,
        access_token: &'a str,
        path_root: Option<&'a str>,
        root_path: &'a str,
    ) -> BoxFuture<'a, Result<ListResult, Error>>;

    fn list_folder_continue<'a>(
        &'a self,
        access_token: &'a str,
        path_root: Option<&'a str>,
        cursor: &'a str,
    ) -> BoxFuture<'a, Result<ListResult, Error>>;

    fn download<'a>(
        &'a self,
        access_token: &'a str,
        path_root: Option<&'a str>,
        path_lower: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, Error>>;
}

#[derive(Clone)]
struct Connection {
    id: Uuid,
    account_id: Uuid,
    root_path: String,
    cursor: Option<String>,
    encrypted_refresh_token: String,
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

#[derive(Debug, Clone)]
pub struct CurrentAccount {
    pub root_namespace_id: String,
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
    let current_account = api.current_account(&access_token).await?;
    let path_root = dropbox_api_path_root(&current_account.root_namespace_id);
    let mut result = match connection.cursor.as_deref() {
        Some(cursor) => {
            api.list_folder_continue(&access_token, Some(path_root.as_str()), cursor)
                .await?
        }
        None => {
            api.list_folder(
                &access_token,
                Some(path_root.as_str()),
                &connection.root_path,
            )
            .await?
        }
    };

    loop {
        for entry in &result.entries {
            apply_entry(
                db,
                api,
                connection,
                &access_token,
                Some(path_root.as_str()),
                entry,
            )
            .await?;
        }
        if !result.has_more {
            persist_cursor(db, connection.id, &result.cursor).await?;
            return Ok(());
        }
        result = api
            .list_folder_continue(&access_token, Some(path_root.as_str()), &result.cursor)
            .await?;
    }
}

async fn apply_entry(
    db: &Database,
    api: &impl DropboxApi,
    connection: &Connection,
    access_token: &str,
    path_root: Option<&str>,
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
                let body = api
                    .download(access_token, path_root, &file.path_lower)
                    .await?;
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
    if path != root && !path.starts_with(&format!("{root}/")) {
        return PathAction::Ignore;
    }
    let rest = path[root.len()..].trim_start_matches('/');
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

async fn due_accounts(db: &Database, poll_sec: u64) -> Result<Vec<Uuid>, Error> {
    let client = db.get().await?;
    let poll_sec = (poll_sec.min(i32::MAX as u64)) as i32;
    let rows = client
        .query(
            "SELECT account_id
             FROM dropbox_connection
             WHERE status = 'active'
               AND sync_state <> 'running'
               AND (
                    sync_state = 'queued'
                    OR last_sync_at IS NULL
                    OR last_sync_at <= NOW() - ($1::int * INTERVAL '1 second')
               )",
            &[&poll_sec],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

async fn claim_account_connection(
    db: &Database,
    account_id: Uuid,
    poll_sec: u64,
) -> Result<Option<Connection>, Error> {
    let client = db.get().await?;
    let poll_sec = (poll_sec.min(i32::MAX as u64)) as i32;
    let row = client
        .query_opt(
            "UPDATE dropbox_connection
             SET sync_state = 'running',
                 sync_started_at = NOW(),
                 updated_at = NOW()
             WHERE account_id = $1
               AND status = 'active'
               AND sync_state <> 'running'
               AND (
                    sync_state = 'queued'
                    OR last_sync_at IS NULL
                    OR last_sync_at <= NOW() - ($2::int * INTERVAL '1 second')
               )
             RETURNING id, account_id, root_path, cursor, encrypted_refresh_token",
            &[&account_id, &poll_sec],
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

async fn persist_cursor(db: &Database, connection_id: Uuid, cursor: &str) -> Result<(), Error> {
    let client = db.get().await?;
    client
        .execute(
            "UPDATE dropbox_connection
             SET cursor = $2,
                 updated_at = NOW()
             WHERE id = $1",
            &[&connection_id, &cursor],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
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

async fn mark_connection_success(db: &Database, connection_id: Uuid) -> Result<(), Error> {
    let client = db.get().await?;
    client
        .execute(
            "UPDATE dropbox_connection
             SET status = 'active',
                 sync_state = 'idle',
                 sync_started_at = NULL,
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
            &[&connection_id],
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
                 sync_state = 'idle',
                 sync_started_at = NULL,
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

fn dropbox_api_path_root(root_namespace_id: &str) -> String {
    serde_json::json!({
        ".tag": "namespace_id",
        "namespace_id": root_namespace_id,
    })
    .to_string()
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
                return Err(dropbox_api_error("dropbox token refresh failed", response).await);
            }
            let body = response.bytes().await.map_err(|_| Error::Database)?;
            serde_json::from_slice::<TokenResponse>(&body)
                .map(|value| value.access_token)
                .map_err(|_| Error::Database)
        })
    }

    fn current_account<'a>(
        &'a self,
        access_token: &'a str,
    ) -> BoxFuture<'a, Result<CurrentAccount, Error>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct CurrentAccountResponse {
                root_info: RootInfo,
            }

            #[derive(Deserialize)]
            struct RootInfo {
                root_namespace_id: String,
            }

            let response = self
                .client
                .post(CURRENT_ACCOUNT_URL)
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(|_| Error::Database)?;
            if !response.status().is_success() {
                return Err(dropbox_api_error("dropbox current account failed", response).await);
            }
            let body = response.bytes().await.map_err(|_| Error::Database)?;
            serde_json::from_slice::<CurrentAccountResponse>(&body)
                .map(|value| CurrentAccount {
                    root_namespace_id: value.root_info.root_namespace_id,
                })
                .map_err(|_| Error::Database)
        })
    }

    fn list_folder<'a>(
        &'a self,
        access_token: &'a str,
        path_root: Option<&'a str>,
        root_path: &'a str,
    ) -> BoxFuture<'a, Result<ListResult, Error>> {
        Box::pin(async move {
            #[derive(Serialize)]
            struct Body<'a> {
                path: &'a str,
                recursive: bool,
            }

            let body = serde_json::to_vec(&Body {
                path: root_path,
                recursive: true,
            })
            .map_err(|_| Error::Internal)?;

            let response = apply_dropbox_path_root(
                self.client
                    .post(LIST_FOLDER_URL)
                    .bearer_auth(access_token)
                    .header("Content-Type", "application/json"),
                path_root,
            )
            .body(body)
            .send()
            .await
            .map_err(|_| Error::Database)?;
            parse_list_response(response).await
        })
    }

    fn list_folder_continue<'a>(
        &'a self,
        access_token: &'a str,
        path_root: Option<&'a str>,
        cursor: &'a str,
    ) -> BoxFuture<'a, Result<ListResult, Error>> {
        Box::pin(async move {
            #[derive(Serialize)]
            struct Body<'a> {
                cursor: &'a str,
            }

            let body = serde_json::to_vec(&Body { cursor }).map_err(|_| Error::Internal)?;

            let response = apply_dropbox_path_root(
                self.client
                    .post(LIST_FOLDER_CONTINUE_URL)
                    .bearer_auth(access_token)
                    .header("Content-Type", "application/json"),
                path_root,
            )
            .body(body)
            .send()
            .await
            .map_err(|_| Error::Database)?;
            parse_list_response(response).await
        })
    }

    fn download<'a>(
        &'a self,
        access_token: &'a str,
        path_root: Option<&'a str>,
        path_lower: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, Error>> {
        Box::pin(async move {
            #[derive(Serialize)]
            struct Arg<'a> {
                path: &'a str,
            }

            let arg =
                serde_json::to_string(&Arg { path: path_lower }).map_err(|_| Error::Internal)?;
            let response = apply_dropbox_path_root(
                self.client
                    .post(DOWNLOAD_URL)
                    .bearer_auth(access_token)
                    .header("Dropbox-API-Arg", arg),
                path_root,
            )
            .send()
            .await
            .map_err(|_| Error::Database)?;
            if !response.status().is_success() {
                return Err(dropbox_api_error("dropbox file download failed", response).await);
            }
            response
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(|_| Error::Database)
        })
    }
}

fn apply_dropbox_path_root(
    request: reqwest::RequestBuilder,
    path_root: Option<&str>,
) -> reqwest::RequestBuilder {
    match path_root {
        Some(value) => request.header("Dropbox-API-Path-Root", value),
        None => request,
    }
}

async fn parse_list_response(response: reqwest::Response) -> Result<ListResult, Error> {
    if !response.status().is_success() {
        return Err(dropbox_api_error("dropbox list folder failed", response).await);
    }
    let body = response.bytes().await.map_err(|_| Error::Database)?;
    serde_json::from_slice::<ApiListResult>(&body)
        .map(Into::into)
        .map_err(|_| Error::Database)
}

async fn dropbox_api_error(prefix: &str, response: reqwest::Response) -> Error {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Error::BadRequest(format!(
        "{prefix}: status={} {}",
        status.as_u16(),
        summarize_dropbox_error_body(&body)
    ))
}

fn summarize_dropbox_error_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "empty response body".into();
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(summary) = value
            .get("error_summary")
            .and_then(Value::as_str)
            .or_else(|| value.get("error").and_then(Value::as_str))
        {
            return truncate_error_summary(summary);
        }
    }

    truncate_error_summary(trimmed)
}

fn truncate_error_summary(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = collapsed.chars();
    let truncated: String = chars.by_ref().take(240).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
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
            map_path("/Logs", "/logs/DEMO-HONDA-ACCORD/a.csv"),
            PathAction::Ingest {
                vin: "DEMO-HONDA-ACCORD".into()
            }
        );
    }

    #[test]
    fn root_csv_is_visible_skip() {
        assert_eq!(
            map_path("/Logs", "/logs/a.csv"),
            PathAction::Skip {
                error: "CSV is directly under Dropbox root; put it in a VIN folder"
            }
        );
    }

    #[test]
    fn nested_csv_is_visible_skip() {
        assert_eq!(
            map_path("/Logs", "/logs/DEMO-HONDA-ACCORD/nested/a.csv"),
            PathAction::Skip {
                error: "Nested Dropbox CSV paths are not supported in v1"
            }
        );
    }

    #[test]
    fn non_csv_is_ignored() {
        assert_eq!(
            map_path(
                "/Apps/OBD Fusion/CsvLogs",
                "/apps/obd fusion/csvlogs/VIN/readme.txt",
            ),
            PathAction::Ignore
        );
    }

    #[test]
    fn outside_selected_root_is_ignored() {
        assert_eq!(
            map_path("/Logs", "/other/DEMO-HONDA-ACCORD/a.csv"),
            PathAction::Ignore
        );
        assert_eq!(
            map_path("/Logs", "/logs2/DEMO-HONDA-ACCORD/a.csv"),
            PathAction::Ignore
        );
    }

    #[test]
    fn summarize_dropbox_error_body_prefers_error_summary() {
        assert_eq!(
            summarize_dropbox_error_body(
                r#"{"error_summary":"path/not_found/..","error":{".tag":"path"}}"#
            ),
            "path/not_found/.."
        );
    }

    #[test]
    fn summarize_dropbox_error_body_collapses_plain_text() {
        assert_eq!(
            summarize_dropbox_error_body("  line one\n line two  "),
            "line one line two"
        );
    }

    #[test]
    fn dropbox_api_path_root_targets_namespace_id() {
        assert_eq!(
            dropbox_api_path_root("12345"),
            r#"{".tag":"namespace_id","namespace_id":"12345"}"#
        );
    }
}
