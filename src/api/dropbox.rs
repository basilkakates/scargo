use crate::config::{DropboxConfig, Settings};
use crate::db::Database;
use crate::Error;
use actix_web::{delete, get, post, web, HttpRequest, HttpResponse};
use aes_gcm_siv::aead::{Aead, KeyInit};
use aes_gcm_siv::{Aes256GcmSiv, Nonce};
use chrono::{DateTime, Duration, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const DROPBOX_AUTHORIZE_URL: &str = "https://www.dropbox.com/oauth2/authorize";
const DROPBOX_TOKEN_URL: &str = "https://api.dropboxapi.com/oauth2/token";
const DEFAULT_REDIRECT_PATH: &str = "/vehicles.html";

#[derive(Deserialize)]
pub(crate) struct OAuthStartRequest {
    redirect_path: Option<String>,
}

#[derive(Serialize)]
struct OAuthStartResponse {
    enabled: bool,
    authorize_url: String,
}

#[derive(Deserialize)]
pub(crate) struct PauseRequest {
    paused: bool,
}

#[derive(Serialize)]
struct ConnectionResponse {
    enabled: bool,
    connected: bool,
    status: Option<String>,
    root_path: String,
    last_sync_at: Option<String>,
    last_success_at: Option<String>,
    latest_error: Option<String>,
    ingested_count: i64,
    duplicate_count: i64,
}

#[derive(Deserialize)]
pub(crate) struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct DropboxTokenResponse {
    access_token: Option<String>,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    account_id: String,
}

#[derive(Clone)]
struct PendingOAuthState {
    account_id: Uuid,
    redirect_path: String,
}

#[derive(Clone)]
struct ConnectionRow {
    status: String,
    root_path: String,
    last_sync_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    latest_error: Option<String>,
    ingested_count: i64,
    duplicate_count: i64,
}

#[post("/dropbox/oauth/start")]
pub(crate) async fn oauth_start(
    db: web::Data<Database>,
    settings: web::Data<Settings>,
    req: HttpRequest,
    body: Option<web::Json<OAuthStartRequest>>,
) -> Result<HttpResponse, Error> {
    let Some(cfg) = settings.dropbox.as_ref() else {
        return Err(Error::BadRequest("dropbox support is disabled".into()));
    };
    let client = db.get().await?;
    let account = require_signed_in_account(&client, &req).await?;
    let redirect_path = validate_redirect_path(
        body.as_ref()
            .and_then(|value| value.redirect_path.as_deref())
            .unwrap_or(DEFAULT_REDIRECT_PATH),
    )?;
    let state_token = new_state_token();
    let state_hash = hash_token(&state_token);
    client
        .execute(
            "DELETE FROM dropbox_oauth_state WHERE account_id = $1",
            &[&account.id],
        )
        .await
        .map_err(|_| Error::Database)?;
    client
        .execute(
            "INSERT INTO dropbox_oauth_state
                (state_hash, account_id, redirect_path, expires_at)
             VALUES ($1, $2, $3, NOW() + INTERVAL '15 minutes')",
            &[&state_hash, &account.id, &redirect_path],
        )
        .await
        .map_err(|_| Error::Database)?;

    Ok(HttpResponse::Ok().json(OAuthStartResponse {
        enabled: true,
        authorize_url: authorize_url(cfg, &state_token),
    }))
}

#[get("/dropbox/oauth/callback")]
pub(crate) async fn oauth_callback(
    db: web::Data<Database>,
    settings: web::Data<Settings>,
    query: web::Query<OAuthCallbackQuery>,
) -> Result<HttpResponse, Error> {
    let Some(cfg) = settings.dropbox.as_ref() else {
        return Err(Error::BadRequest("dropbox support is disabled".into()));
    };
    if let Some(err) = query.error.as_deref() {
        return Err(Error::BadRequest(format!("dropbox oauth error: {err}")));
    }
    let code = query
        .code
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::BadRequest("missing dropbox oauth code".into()))?;
    let state = query
        .state
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::BadRequest("missing dropbox oauth state".into()))?;

    let client = db.get().await?;
    let pending = consume_oauth_state(&client, state).await?;
    let token = exchange_code(cfg, code).await?;
    let refresh_token = token
        .refresh_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::BadRequest("dropbox oauth response missing refresh token".into()))?;
    let encrypted_refresh = encrypt_token(&cfg.token_encryption_key, refresh_token)?;
    let encrypted_access = token
        .access_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| encrypt_token(&cfg.token_encryption_key, value))
        .transpose()?;
    let access_token_expires_at = token
        .expires_in
        .map(|seconds| Utc::now() + Duration::seconds(seconds.max(0)));
    let connection_id = Uuid::new_v4();

    client
        .execute(
            "INSERT INTO dropbox_connection
                (id, account_id, dropbox_account_id, root_path, encrypted_refresh_token,
                 encrypted_access_token, access_token_expires_at, cursor, status,
                 last_sync_at, last_success_at, latest_error, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, 'active', NULL, NULL, NULL, NOW(), NOW())
             ON CONFLICT (account_id)
             DO UPDATE SET
                dropbox_account_id = EXCLUDED.dropbox_account_id,
                root_path = EXCLUDED.root_path,
                encrypted_refresh_token = EXCLUDED.encrypted_refresh_token,
                encrypted_access_token = EXCLUDED.encrypted_access_token,
                access_token_expires_at = EXCLUDED.access_token_expires_at,
                cursor = NULL,
                status = 'active',
                latest_error = NULL,
                updated_at = NOW()",
            &[
                &connection_id,
                &pending.account_id,
                &token.account_id,
                &cfg.root_path,
                &encrypted_refresh,
                &encrypted_access,
                &access_token_expires_at,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;

    Ok(HttpResponse::Found()
        .append_header((
            "Location",
            format!("{}?dropbox=connected", pending.redirect_path),
        ))
        .finish())
}

#[get("/dropbox/connection")]
pub(crate) async fn connection(
    db: web::Data<Database>,
    settings: web::Data<Settings>,
    req: HttpRequest,
) -> Result<HttpResponse, Error> {
    let root_path = settings
        .dropbox
        .as_ref()
        .map(|cfg| cfg.root_path)
        .unwrap_or("/OBD Fusion/CsvLogs");
    let Some(cfg) = settings.dropbox.as_ref() else {
        return Ok(HttpResponse::Ok().json(ConnectionResponse {
            enabled: false,
            connected: false,
            status: None,
            root_path: root_path.to_string(),
            last_sync_at: None,
            last_success_at: None,
            latest_error: None,
            ingested_count: 0,
            duplicate_count: 0,
        }));
    };

    let client = db.get().await?;
    let account = require_signed_in_account(&client, &req).await?;
    let row = load_connection(&client, account.id).await?;
    Ok(HttpResponse::Ok().json(connection_response(Some(cfg), row)))
}

#[post("/dropbox/connection/pause")]
pub(crate) async fn pause_connection(
    db: web::Data<Database>,
    settings: web::Data<Settings>,
    req: HttpRequest,
    body: web::Json<PauseRequest>,
) -> Result<HttpResponse, Error> {
    let Some(cfg) = settings.dropbox.as_ref() else {
        return Err(Error::BadRequest("dropbox support is disabled".into()));
    };
    let client = db.get().await?;
    let account = require_signed_in_account(&client, &req).await?;
    let status = if body.paused { "paused" } else { "active" };
    let updated = client
        .execute(
            "UPDATE dropbox_connection
             SET status = $2, updated_at = NOW()
             WHERE account_id = $1",
            &[&account.id, &status],
        )
        .await
        .map_err(|_| Error::Database)?;
    if updated == 0 {
        return Err(Error::NotFound("dropbox connection".into()));
    }
    let row = load_connection(&client, account.id).await?;
    Ok(HttpResponse::Ok().json(connection_response(Some(cfg), row)))
}

#[delete("/dropbox/connection")]
pub(crate) async fn delete_connection(
    db: web::Data<Database>,
    settings: web::Data<Settings>,
    req: HttpRequest,
) -> Result<HttpResponse, Error> {
    let Some(_cfg) = settings.dropbox.as_ref() else {
        return Err(Error::BadRequest("dropbox support is disabled".into()));
    };
    let client = db.get().await?;
    let account = require_signed_in_account(&client, &req).await?;
    let deleted = client
        .execute(
            "DELETE FROM dropbox_connection WHERE account_id = $1",
            &[&account.id],
        )
        .await
        .map_err(|_| Error::Database)?;
    if deleted == 0 {
        return Err(Error::NotFound("dropbox connection".into()));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({"deleted": true})))
}

fn authorize_url(cfg: &DropboxConfig, state: &str) -> String {
    format!(
        "{DROPBOX_AUTHORIZE_URL}?client_id={}&response_type=code&token_access_type=offline&state={}&redirect_uri={}",
        urlencoding::encode(&cfg.app_key),
        urlencoding::encode(state),
        urlencoding::encode(&callback_url(cfg)),
    )
}

fn callback_url(cfg: &DropboxConfig) -> String {
    format!("{}/api/dropbox/oauth/callback", cfg.base_url)
}

async fn exchange_code(cfg: &DropboxConfig, code: &str) -> Result<DropboxTokenResponse, Error> {
    let redirect_uri = callback_url(cfg);
    let response = reqwest::Client::new()
        .post(DROPBOX_TOKEN_URL)
        .form(&[
            ("code", code),
            ("grant_type", "authorization_code"),
            ("client_id", cfg.app_key.as_str()),
            ("client_secret", cfg.app_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|_| Error::Database)?;
    if response.status() != StatusCode::OK {
        return Err(Error::BadRequest("dropbox token exchange failed".into()));
    }
    response.json().await.map_err(|_| Error::Database)
}

async fn require_signed_in_account(
    client: &tokio_postgres::Client,
    req: &HttpRequest,
) -> Result<super::privacy::Account, Error> {
    let account = super::privacy::session_account(client, req).await?;
    if account.is_guest {
        return Err(Error::BadRequest(
            "guest accounts cannot manage dropbox".into(),
        ));
    }
    Ok(account)
}

async fn consume_oauth_state(
    client: &tokio_postgres::Client,
    state: &str,
) -> Result<PendingOAuthState, Error> {
    let row = client
        .query_opt(
            "DELETE FROM dropbox_oauth_state
             WHERE state_hash = $1
             RETURNING account_id, redirect_path, expires_at",
            &[&hash_token(state)],
        )
        .await
        .map_err(|_| Error::Database)?
        .ok_or_else(|| Error::BadRequest("invalid dropbox oauth state".into()))?;
    let expires_at: DateTime<Utc> = row.get(2);
    validate_state_expiry(expires_at)?;
    Ok(PendingOAuthState {
        account_id: row.get(0),
        redirect_path: row.get(1),
    })
}

fn validate_state_expiry(expires_at: DateTime<Utc>) -> Result<(), Error> {
    if expires_at > Utc::now() {
        Ok(())
    } else {
        Err(Error::BadRequest("expired dropbox oauth state".into()))
    }
}

async fn load_connection(
    client: &tokio_postgres::Client,
    account_id: Uuid,
) -> Result<Option<ConnectionRow>, Error> {
    let row = client
        .query_opt(
            "SELECT c.status,
                    c.root_path,
                    c.last_sync_at,
                    c.last_success_at,
                    c.latest_error,
                    COALESCE(COUNT(f.id) FILTER (WHERE f.ingested_at IS NOT NULL), 0)::BIGINT,
                    COALESCE(COUNT(f.id) FILTER (WHERE f.duplicate), 0)::BIGINT
             FROM dropbox_connection c
             LEFT JOIN dropbox_ingest_file f
               ON f.connection_id = c.id
             WHERE c.account_id = $1
             GROUP BY c.id, c.status, c.root_path, c.last_sync_at, c.last_success_at, c.latest_error",
            &[&account_id],
        )
        .await
        .map_err(|_| Error::Database)?;

    Ok(row.map(|row| ConnectionRow {
        status: row.get(0),
        root_path: row.get(1),
        last_sync_at: row.get(2),
        last_success_at: row.get(3),
        latest_error: row.get(4),
        ingested_count: row.get(5),
        duplicate_count: row.get(6),
    }))
}

fn connection_response(
    cfg: Option<&DropboxConfig>,
    row: Option<ConnectionRow>,
) -> ConnectionResponse {
    match row {
        Some(row) => ConnectionResponse {
            enabled: cfg.is_some(),
            connected: true,
            status: Some(row.status),
            root_path: row.root_path,
            last_sync_at: row.last_sync_at.map(|value| value.to_rfc3339()),
            last_success_at: row.last_success_at.map(|value| value.to_rfc3339()),
            latest_error: row.latest_error,
            ingested_count: row.ingested_count,
            duplicate_count: row.duplicate_count,
        },
        None => ConnectionResponse {
            enabled: cfg.is_some(),
            connected: false,
            status: None,
            root_path: cfg
                .map(|value| value.root_path)
                .unwrap_or("/OBD Fusion/CsvLogs")
                .to_string(),
            last_sync_at: None,
            last_success_at: None,
            latest_error: None,
            ingested_count: 0,
            duplicate_count: 0,
        },
    }
}

fn validate_redirect_path(value: &str) -> Result<String, Error> {
    let value = value.trim();
    if value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains('\n')
        && !value.contains('\r')
    {
        Ok(value.to_string())
    } else {
        Err(Error::BadRequest(
            "redirect_path must be an app-relative path".into(),
        ))
    }
}

fn new_state_token() -> String {
    format!(
        "dropbox_state_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing to string cannot fail");
    }
    out
}

pub(crate) fn encrypt_token(key: &[u8; 32], plaintext: &str) -> Result<Vec<u8>, Error> {
    let cipher = Aes256GcmSiv::new_from_slice(key).map_err(|_| Error::Internal)?;
    let nonce_bytes = *Uuid::new_v4().as_bytes();
    let nonce = Nonce::from_slice(&nonce_bytes[..12]);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| Error::Internal)?;
    let mut out = nonce_bytes[..12].to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub(crate) fn decrypt_token(key: &[u8; 32], ciphertext: &[u8]) -> Result<String, Error> {
    if ciphertext.len() < 13 {
        return Err(Error::BadRequest("invalid encrypted token".into()));
    }
    let cipher = Aes256GcmSiv::new_from_slice(key).map_err(|_| Error::Internal)?;
    let nonce = Nonce::from_slice(&ciphertext[..12]);
    let plaintext = cipher
        .decrypt(nonce, &ciphertext[12..])
        .map_err(|_| Error::BadRequest("invalid encrypted token".into()))?;
    String::from_utf8(plaintext).map_err(|_| Error::BadRequest("invalid encrypted token".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn token_encryption_round_trip() {
        let encrypted = encrypt_token(&key(7), "refresh-token").unwrap();
        assert_eq!(decrypt_token(&key(7), &encrypted).unwrap(), "refresh-token");
    }

    #[test]
    fn token_encryption_rejects_wrong_key() {
        let encrypted = encrypt_token(&key(7), "refresh-token").unwrap();
        assert!(decrypt_token(&key(8), &encrypted).is_err());
    }

    #[test]
    fn redirect_path_must_stay_relative() {
        assert_eq!(
            validate_redirect_path("/vehicles.html?tab=dropbox").unwrap(),
            "/vehicles.html?tab=dropbox"
        );
        assert!(validate_redirect_path("https://example.com").is_err());
        assert!(validate_redirect_path("//example.com").is_err());
    }

    #[test]
    fn oauth_state_expiry_rejects_past_values() {
        assert!(validate_state_expiry(Utc::now() + Duration::minutes(1)).is_ok());
        assert!(validate_state_expiry(Utc::now() - Duration::seconds(1)).is_err());
    }

    #[test]
    fn authorize_url_uses_callback_and_state() {
        let cfg = DropboxConfig {
            app_key: "app-key".into(),
            app_secret: "secret".into(),
            base_url: "http://localhost:8080".into(),
            token_encryption_key: key(1),
            poll_sec: 60,
            root_path: "/OBD Fusion/CsvLogs",
        };
        let url = authorize_url(&cfg, "state123");
        assert!(url.contains("client_id=app-key"));
        assert!(url.contains("state=state123"));
        assert!(url.contains(
            "redirect_uri=http%3A%2F%2Flocalhost%3A8080%2Fapi%2Fdropbox%2Foauth%2Fcallback"
        ));
    }

    #[test]
    fn connection_response_defaults_to_disconnected() {
        let response = connection_response(None, None);
        assert!(!response.enabled);
        assert!(!response.connected);
        assert_eq!(response.root_path, "/OBD Fusion/CsvLogs");
    }
}
