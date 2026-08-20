//! Shared admin API error envelope.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// Admin API error response body.
#[derive(Debug, Serialize)]
pub struct AdminErrorResponse {
    /// Optional request ID for correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Error details.
    pub error: AdminErrorDetail,
}

/// Error details within an admin API error response.
#[derive(Debug, Serialize)]
pub struct AdminErrorDetail {
    /// Machine-readable error code.
    pub code: &'static str,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Admin API error type that can be converted into an HTTP response.
#[derive(Debug)]
pub struct AdminError {
    /// HTTP status code for the response.
    status: StatusCode,
    /// Machine-readable error code.
    code: &'static str,
    /// Human-readable error message.
    message: String,
    /// Optional extra details for debugging.
    details: Option<String>,
    /// Optional request ID for correlation.
    request_id: Option<String>,
}

impl AdminError {
    /// Create a new admin error.
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details: None,
            request_id: None,
        }
    }

    /// Create a 400 Bad Request error.
    #[must_use]
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    /// Create a 401 Unauthorized error.
    #[must_use]
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    /// Create a 403 Forbidden error.
    #[must_use]
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    /// Create a 404 Not Found error.
    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    /// Create a 409 Conflict error.
    #[must_use]
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    /// Create a 429 Too Many Requests error.
    #[must_use]
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", message)
    }

    /// Create a 500 Internal Server Error.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }

    /// Attach a request ID to the error response.
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        let body = AdminErrorResponse {
            request_id: self.request_id,
            error: AdminErrorDetail {
                code: self.code,
                message: self.message,
                details: self.details,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

/// Result type for admin API handlers.
pub type AdminResult<T> = Result<T, AdminError>;
