use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;
use tracing::error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, response) = match self {
            AppError::Unauthorized(message) => (
                StatusCode::UNAUTHORIZED,
                ErrorResponse {
                    error: "unauthorized".to_string(),
                    message,
                },
            ),
            AppError::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse {
                    error: "bad_request".to_string(),
                    message,
                },
            ),
            AppError::NotFound(message) => (
                StatusCode::NOT_FOUND,
                ErrorResponse {
                    error: "not_found".to_string(),
                    message,
                },
            ),
            AppError::Database(source) => {
                error!(error = %source, "Database request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorResponse {
                        error: "internal_error".to_string(),
                        message: "A database error occurred".to_string(),
                    },
                )
            }
            AppError::Io(source) => {
                error!(error = %source, "File system request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorResponse {
                        error: "internal_error".to_string(),
                        message: "A file system error occurred".to_string(),
                    },
                )
            }
            AppError::Internal(message) => {
                error!(error = %message, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorResponse {
                        error: "internal_error".to_string(),
                        message,
                    },
                )
            }
        };

        (status, Json(response)).into_response()
    }
}

impl From<argon2::password_hash::Error> for AppError {
    fn from(value: argon2::password_hash::Error) -> Self {
        Self::Internal(format!("Password hashing failed: {value}"))
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(value: sqlx::migrate::MigrateError) -> Self {
        Self::Internal(format!("Database migration failed: {value}"))
    }
}
