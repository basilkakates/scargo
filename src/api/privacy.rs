use crate::Error;
use actix_web::http::header;
use actix_web::HttpRequest;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "scargo_session";
pub const GUEST_USERNAME: &str = "guest";
pub const LOCAL_DEV_USER_KEY: &str = "local-dev";
const USER_KEY_HEADER: &str = "x-scargo-user-key";
const API_TOKEN_PREFIX: &str = "scargo_";

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub is_guest: bool,
}

pub async fn account_id(client: &tokio_postgres::Client, req: &HttpRequest) -> Result<Uuid, Error> {
    Ok(resolve_account(client, req).await?.id)
}

pub async fn resolve_account(
    client: &tokio_postgres::Client,
    req: &HttpRequest,
) -> Result<Account, Error> {
    if let Some(token) = session_token(req) {
        if let Some(account) = account_from_session(client, &token).await? {
            return Ok(account);
        }
    }

    if let Some(token) = bearer_token(req) {
        if let Some(account) = account_from_api_token(client, token).await? {
            return Ok(account);
        }
        return Err(Error::Unauthorized);
    }

    if let Some(key) = legacy_user_key(req) {
        if guest_enabled() {
            return ensure_legacy_account(client, key).await;
        }
        return Err(Error::Unauthorized);
    }

    if guest_enabled() {
        return ensure_guest_account(client).await;
    }

    Err(Error::Unauthorized)
}

pub async fn session_account(
    client: &tokio_postgres::Client,
    req: &HttpRequest,
) -> Result<Account, Error> {
    let Some(token) = session_token(req) else {
        return Err(Error::Unauthorized);
    };
    account_from_session(client, &token)
        .await?
        .ok_or(Error::Unauthorized)
}

pub async fn ensure_guest_account(client: &tokio_postgres::Client) -> Result<Account, Error> {
    let id = account_id_from_user_key(Some(LOCAL_DEV_USER_KEY));
    client
        .execute(
            "INSERT INTO account (id, username, label, display_name, is_guest)
             VALUES ($1, $2, $3, $4, TRUE)
             ON CONFLICT (id) DO UPDATE SET
                username = COALESCE(account.username, EXCLUDED.username),
                label = EXCLUDED.label,
                display_name = COALESCE(NULLIF(account.display_name, ''), EXCLUDED.display_name),
                is_guest = TRUE",
            &[&id, &GUEST_USERNAME, &LOCAL_DEV_USER_KEY, &"Guest"],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(Account {
        id,
        username: GUEST_USERNAME.into(),
        display_name: "Guest".into(),
        is_guest: true,
    })
}

pub async fn create_session(
    client: &tokio_postgres::Client,
    account_id: Uuid,
) -> Result<String, Error> {
    let token = new_token("");
    let token_hash = hash_token(&token);
    client
        .execute(
            "INSERT INTO account_session (token_hash, account_id, expires_at)
             VALUES ($1, $2, NOW() + INTERVAL '30 days')",
            &[&token_hash, &account_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(token)
}

pub async fn create_api_token(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    label: &str,
) -> Result<String, Error> {
    let token = new_token(API_TOKEN_PREFIX);
    let token_hash = hash_token(&token);
    client
        .execute(
            "INSERT INTO account_api_token (token_hash, account_id, label)
             VALUES ($1, $2, $3)",
            &[&token_hash, &account_id, &label],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(token)
}

pub async fn api_token_account_id(
    client: &tokio_postgres::Client,
    token: &str,
) -> Result<Option<Uuid>, Error> {
    Ok(account_from_api_token(client, token)
        .await?
        .map(|account| account.id))
}

pub async fn find_account_by_username(
    client: &tokio_postgres::Client,
    username: &str,
) -> Result<Option<(Account, Option<String>)>, Error> {
    let row = client
        .query_opt(
            "SELECT id,
                    COALESCE(username, ''),
                    COALESCE(NULLIF(display_name, ''), username, label, 'Account'),
                    is_guest,
                    password_hash
             FROM account
             WHERE username = $1",
            &[&username],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row.map(|row| {
        (
            Account {
                id: row.get(0),
                username: row.get(1),
                display_name: row.get(2),
                is_guest: row.get(3),
            },
            row.get(4),
        )
    }))
}

pub async fn insert_account(
    client: &tokio_postgres::Client,
    username: &str,
    password_hash: &str,
) -> Result<Account, Error> {
    let id = Uuid::new_v4();
    let row = client
        .query_one(
            "INSERT INTO account (id, username, label, display_name, password_hash)
             VALUES ($1, $2, $2, $2, $3)
             RETURNING id, username, display_name, is_guest",
            &[&id, &username, &password_hash],
        )
        .await
        .map_err(|_| Error::BadRequest("username is already registered".into()))?;
    Ok(Account {
        id: row.get(0),
        username: row.get(1),
        display_name: row.get(2),
        is_guest: row.get(3),
    })
}

pub async fn delete_session(
    client: &tokio_postgres::Client,
    req: &HttpRequest,
) -> Result<(), Error> {
    if let Some(token) = session_token(req) {
        client
            .execute(
                "DELETE FROM account_session WHERE token_hash = $1",
                &[&hash_token(&token)],
            )
            .await
            .map_err(|_| Error::Database)?;
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, Error> {
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|_| Error::Internal)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| Error::Internal)
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn account_id_from_user_key(key: Option<&str>) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        key.unwrap_or(LOCAL_DEV_USER_KEY).as_bytes(),
    )
}

pub async fn ensure_account(
    client: &tokio_postgres::Client,
    account_id: Uuid,
) -> Result<(), Error> {
    client
        .execute(
            "INSERT INTO account (id, label, display_name)
             VALUES ($1, $2, $2)
             ON CONFLICT (id) DO NOTHING",
            &[&account_id, &LOCAL_DEV_USER_KEY],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

pub async fn set_exact_vin_share_preference(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    vehicle_id: Uuid,
    enabled: bool,
) -> Result<(), Error> {
    client
        .execute(
            "INSERT INTO account_vehicle_profile (account_id, vehicle_id, exact_vin_share_enabled)
             VALUES ($1, $2, $3)
             ON CONFLICT (account_id, vehicle_id)
             DO UPDATE SET
                exact_vin_share_enabled = EXCLUDED.exact_vin_share_enabled,
                updated_at = NOW()",
            &[&account_id, &vehicle_id, &enabled],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

pub async fn exact_vin_share_preference(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    vehicle_id: Uuid,
) -> Result<bool, Error> {
    let row = client
        .query_opt(
            "SELECT exact_vin_share_enabled
             FROM account_vehicle_profile
             WHERE account_id = $1 AND vehicle_id = $2",
            &[&account_id, &vehicle_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row.map(|row| row.get(0)).unwrap_or(false))
}

pub async fn link_upload_to_account(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    upload_id: Uuid,
    vehicle_id: Uuid,
) -> Result<(), Error> {
    let exact_share = exact_vin_share_preference(client, account_id, vehicle_id).await?;
    client
        .execute(
            "INSERT INTO account_vehicle_upload
                (account_id, upload_id, vehicle_id, private_access, exact_vin_share_enabled, linked_at, access_revoked_at)
             VALUES ($1, $2, $3, TRUE, $4, NOW(), NULL)
             ON CONFLICT (account_id, upload_id)
             DO UPDATE SET
                vehicle_id = EXCLUDED.vehicle_id,
                private_access = TRUE,
                exact_vin_share_enabled = EXCLUDED.exact_vin_share_enabled,
                linked_at = NOW(),
                access_revoked_at = NULL",
            &[&account_id, &upload_id, &vehicle_id, &exact_share],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

pub async fn can_access_vehicle(
    client: &tokio_postgres::Client,
    vehicle_id: uuid::Uuid,
    account_id: Uuid,
) -> Result<bool, Error> {
    let row = client
        .query_one(
            "SELECT EXISTS (
                SELECT 1
                FROM account_vehicle_upload
                WHERE vehicle_id = $1::uuid
                  AND account_id = $2
                  AND private_access
             )",
            &[&vehicle_id, &account_id],
        )
        .await
        .map_err(|_| Error::Database)?;

    Ok(row.get(0))
}

pub async fn revoke_vehicle_private_access(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    vehicle_id: Uuid,
) -> Result<u64, Error> {
    let updated = client
        .execute(
            "UPDATE account_vehicle_upload
             SET private_access = FALSE,
                 access_revoked_at = NOW()
             WHERE account_id = $1
               AND vehicle_id = $2
               AND private_access",
            &[&account_id, &vehicle_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    client
        .execute(
            "DELETE FROM account_vehicle_profile
             WHERE account_id = $1 AND vehicle_id = $2",
            &[&account_id, &vehicle_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(updated)
}

pub async fn set_vehicle_exact_vin_sharing(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    vehicle_id: Uuid,
    enabled: bool,
) -> Result<(), Error> {
    set_exact_vin_share_preference(client, account_id, vehicle_id, enabled).await?;
    client
        .execute(
            "UPDATE account_vehicle_upload
             SET exact_vin_share_enabled = $3
             WHERE account_id = $1
               AND vehicle_id = $2",
            &[&account_id, &vehicle_id, &enabled],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

async fn account_from_session(
    client: &tokio_postgres::Client,
    token: &str,
) -> Result<Option<Account>, Error> {
    let token_hash = hash_token(token);
    let row = client
        .query_opt(
            "SELECT a.id,
                    COALESCE(a.username, ''),
                    COALESCE(NULLIF(a.display_name, ''), a.username, a.label, 'Account'),
                    a.is_guest
             FROM account_session s
             JOIN account a ON a.id = s.account_id
             WHERE s.token_hash = $1 AND s.expires_at > NOW()",
            &[&token_hash],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row.map(account_from_row))
}

async fn account_from_api_token(
    client: &tokio_postgres::Client,
    token: &str,
) -> Result<Option<Account>, Error> {
    let token_hash = hash_token(token);
    let row = client
        .query_opt(
            "SELECT a.id,
                    COALESCE(a.username, ''),
                    COALESCE(NULLIF(a.display_name, ''), a.username, a.label, 'Account'),
                    a.is_guest
             FROM account_api_token t
             JOIN account a ON a.id = t.account_id
             WHERE t.token_hash = $1 AND t.revoked_at IS NULL",
            &[&token_hash],
        )
        .await
        .map_err(|_| Error::Database)?;
    if row.is_some() {
        client
            .execute(
                "UPDATE account_api_token SET last_used_at = NOW() WHERE token_hash = $1",
                &[&token_hash],
            )
            .await
            .map_err(|_| Error::Database)?;
    }
    Ok(row.map(account_from_row))
}

async fn ensure_legacy_account(
    client: &tokio_postgres::Client,
    user_key: &str,
) -> Result<Account, Error> {
    let key = user_key.trim();
    if key.is_empty() || key == LOCAL_DEV_USER_KEY || key == GUEST_USERNAME {
        return ensure_guest_account(client).await;
    }

    let id = account_id_from_user_key(Some(key));
    client
        .execute(
            "INSERT INTO account (id, label, display_name)
             VALUES ($1, $2, $2)
             ON CONFLICT (id) DO UPDATE SET
                label = EXCLUDED.label,
                display_name = COALESCE(NULLIF(account.display_name, ''), EXCLUDED.display_name)",
            &[&id, &key],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(Account {
        id,
        username: String::new(),
        display_name: key.into(),
        is_guest: false,
    })
}

fn account_from_row(row: tokio_postgres::Row) -> Account {
    Account {
        id: row.get(0),
        username: row.get(1),
        display_name: row.get(2),
        is_guest: row.get(3),
    }
}

fn session_token(req: &HttpRequest) -> Option<String> {
    req.cookie(SESSION_COOKIE)
        .map(|cookie| cookie.value().trim().to_string())
        .filter(|token| !token.is_empty())
}

fn bearer_token(req: &HttpRequest) -> Option<&str> {
    let value = req
        .headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .trim();
    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn legacy_user_key(req: &HttpRequest) -> Option<&str> {
    req.headers()
        .get(USER_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn guest_enabled() -> bool {
    match std::env::var("SCARGO_ENABLE_GUEST")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("1" | "true" | "yes") => return true,
        Some("0" | "false" | "no") => return false,
        _ => {}
    }
    std::env::var("SCARGO_ENV")
        .map(|env| env.trim().eq_ignore_ascii_case("production"))
        .map(|production| !production)
        .unwrap_or(true)
}

pub fn manual_public_approval_enabled() -> bool {
    std::env::var("SCARGO_ENV")
        .map(|env| {
            let env = env.trim();
            env.eq_ignore_ascii_case("dev") || env.eq_ignore_ascii_case("test")
        })
        .unwrap_or(true)
}

fn new_token(prefix: &str) -> String {
    format!(
        "{prefix}{}{}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn local_dev_account_id_stays_stable() {
        assert_eq!(
            account_id_from_user_key(Some(LOCAL_DEV_USER_KEY)).to_string(),
            "889705d1-e9c0-53ca-9415-37f0afc024ff"
        );
    }

    #[test]
    fn password_hash_verifies_only_original_password() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password(&hash, "correct horse battery staple"));
        assert!(!verify_password(&hash, "wrong password"));
    }

    #[test]
    fn generated_api_tokens_are_prefixed_and_hashable() {
        let token = new_token(API_TOKEN_PREFIX);
        assert!(token.starts_with(API_TOKEN_PREFIX));
        assert_ne!(hash_token(&token), hash_token("other"));
    }

    #[test]
    fn manual_public_approval_tracks_env() {
        with_env("SCARGO_ENV", "dev", || {
            assert!(manual_public_approval_enabled());
        });
        with_env("SCARGO_ENV", "test", || {
            assert!(manual_public_approval_enabled());
        });
        with_env("SCARGO_ENV", "production", || {
            assert!(!manual_public_approval_enabled());
        });
        without_env("SCARGO_ENV", || {
            assert!(manual_public_approval_enabled());
        });
    }

    fn with_env(name: &str, value: &str, test: impl FnOnce()) {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock poisoned");
        let original = std::env::var_os(name);
        std::env::set_var(name, value);
        test();
        restore_env(name, original);
    }

    fn without_env(name: &str, test: impl FnOnce()) {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock poisoned");
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
