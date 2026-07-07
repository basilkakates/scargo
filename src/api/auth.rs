use crate::db::Database;
use crate::Error;
use actix_web::cookie::{Cookie, SameSite};
use actix_web::{get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct TokenRequest {
    label: Option<String>,
}

#[derive(Serialize)]
struct AuthResponse {
    account: super::privacy::Account,
    #[serde(skip_serializing_if = "Option::is_none")]
    upload_token: Option<String>,
}

#[derive(Serialize)]
struct AuthCapabilities {
    approve_pending_public_stats: bool,
}

#[derive(Serialize)]
struct AuthMeResponse {
    account: super::privacy::Account,
    capabilities: AuthCapabilities,
}

#[post("/auth/register")]
async fn register(
    db: web::Data<Database>,
    credentials: web::Json<Credentials>,
) -> Result<HttpResponse, Error> {
    let username = normalize_username(&credentials.username)?;
    validate_password(&credentials.password)?;
    let password_hash = super::privacy::hash_password(&credentials.password)?;
    let client = db.get().await?;
    let account = super::privacy::insert_account(&client, &username, &password_hash).await?;
    let session = super::privacy::create_session(&client, account.id).await?;
    let upload_token = super::privacy::create_api_token(&client, account.id, "default").await?;
    Ok(HttpResponse::Ok()
        .cookie(session_cookie(&session))
        .json(AuthResponse {
            account,
            upload_token: Some(upload_token),
        }))
}

#[post("/auth/login")]
async fn login(
    db: web::Data<Database>,
    credentials: web::Json<Credentials>,
) -> Result<HttpResponse, Error> {
    let username = normalize_username(&credentials.username)?;
    let client = db.get().await?;
    let Some((account, Some(password_hash))) =
        super::privacy::find_account_by_username(&client, &username).await?
    else {
        return Err(Error::Unauthorized);
    };
    if !super::privacy::verify_password(&password_hash, &credentials.password) {
        return Err(Error::Unauthorized);
    }
    let session = super::privacy::create_session(&client, account.id).await?;
    Ok(HttpResponse::Ok()
        .cookie(session_cookie(&session))
        .json(AuthResponse {
            account,
            upload_token: None,
        }))
}

#[post("/auth/logout")]
async fn logout(db: web::Data<Database>, req: HttpRequest) -> Result<HttpResponse, Error> {
    let client = db.get().await?;
    super::privacy::delete_session(&client, &req).await?;
    let mut cookie = Cookie::build(super::privacy::SESSION_COOKIE, "")
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .finish();
    cookie.make_removal();
    Ok(HttpResponse::Ok()
        .cookie(cookie)
        .json(serde_json::json!({"ok": true})))
}

#[get("/auth/me")]
async fn me(db: web::Data<Database>, req: HttpRequest) -> Result<HttpResponse, Error> {
    let client = db.get().await?;
    let account = super::privacy::resolve_account(&client, &req).await?;
    Ok(HttpResponse::Ok().json(AuthMeResponse {
        capabilities: AuthCapabilities {
            approve_pending_public_stats: super::privacy::manual_public_approval_enabled()
                && !account.is_guest,
        },
        account,
    }))
}

#[post("/auth/tokens")]
async fn create_token(
    db: web::Data<Database>,
    req: HttpRequest,
    token: web::Json<TokenRequest>,
) -> Result<HttpResponse, Error> {
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::BadRequest(
            "guest accounts do not issue upload tokens".into(),
        ));
    }
    let label = token
        .label
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("upload");
    let upload_token = super::privacy::create_api_token(&client, account.id, label).await?;
    Ok(HttpResponse::Ok().json(AuthResponse {
        account,
        upload_token: Some(upload_token),
    }))
}

fn session_cookie(token: &str) -> Cookie<'_> {
    Cookie::build(super::privacy::SESSION_COOKIE, token.to_string())
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .finish()
}

fn normalize_username(username: &str) -> Result<String, Error> {
    let username = username.trim().to_ascii_lowercase();
    let valid = username.len() >= 3
        && username.len() <= 80
        && username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'@'));
    if valid {
        Ok(username)
    } else {
        Err(Error::BadRequest(
            "username must be 3-80 letters, numbers, dots, dashes, underscores, or @".into(),
        ))
    }
}

fn validate_password(password: &str) -> Result<(), Error> {
    if password.len() >= 8 {
        Ok(())
    } else {
        Err(Error::BadRequest(
            "password must be at least 8 characters".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn username_validation_is_strict_and_simple() {
        assert_eq!(
            super::normalize_username(" User.Name ").unwrap(),
            "user.name"
        );
        assert!(super::normalize_username("no").is_err());
        assert!(super::normalize_username("bad space").is_err());
    }

    #[test]
    fn password_validation_requires_minimum_length() {
        assert!(super::validate_password("12345678").is_ok());
        assert!(super::validate_password("1234567").is_err());
    }
}
