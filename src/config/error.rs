use actix_web::HttpResponse;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
pub enum Error {
    #[error("internal server error")]
    Internal,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("csv parse error")]
    CsvParse,

    #[error("database error")]
    Database,
}

impl actix_web::ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        match self {
            Error::Internal => HttpResponse::InternalServerError().json(serde_json::json!({
                "error": self.to_string()
            })),
            Error::NotFound(_) => HttpResponse::NotFound().json(serde_json::json!({
                "error": self.to_string()
            })),
            Error::BadRequest(_) => HttpResponse::BadRequest().json(serde_json::json!({
                "error": self.to_string()
            })),
            Error::Unauthorized => HttpResponse::Unauthorized().json(serde_json::json!({
                "error": self.to_string()
            })),
            Error::CsvParse => HttpResponse::BadRequest().json(serde_json::json!({
                "error": self.to_string()
            })),
            Error::Database => HttpResponse::InternalServerError().json(serde_json::json!({
                "error": self.to_string()
            })),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::ResponseError;

    #[test]
    fn implements_response_error() {
        let _ = Error::Internal.error_response();
    }
}
