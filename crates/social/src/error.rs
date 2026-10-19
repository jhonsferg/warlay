use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use tracing::{error, warn};

/// Every API failure: an HTTP status, a stable machine-readable `code` the
/// client can branch on, and a human-readable (Spanish) message.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "sesión inválida o expirada")
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn internal(error: impl std::fmt::Display) -> Self {
        error!(%error, "internal error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "error interno del servidor",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": self.code, "message": self.message })),
        )
            .into_response()
    }
}

impl ApiError {
    /// A backing store is momentarily out of reach (restarting, pool
    /// exhausted): a retryable 503 rather than an internal error.
    fn unavailable(error: impl std::fmt::Display) -> Self {
        warn!(%error, "backing store unavailable");
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "el servicio no está disponible en este momento, intenta de nuevo",
        )
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::PoolTimedOut | sqlx::Error::Io(_) | sqlx::Error::PoolClosed => Self::unavailable(error),
            other => Self::internal(other),
        }
    }
}

impl From<redis::RedisError> for ApiError {
    fn from(error: redis::RedisError) -> Self {
        let transient = error.is_io_error()
            || error.is_connection_dropped()
            || error.is_connection_refusal()
            || error.is_timeout()
            || error.kind() == redis::ErrorKind::BusyLoadingError;
        if transient {
            Self::unavailable(error)
        } else {
            Self::internal(error)
        }
    }
}
