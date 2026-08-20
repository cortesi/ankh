//! Public API error envelope and mountable Ankh router.

mod auth_routes;
mod device_routes;
mod org_routes;

use std::{error::Error, fmt};

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing,
};
use serde::Serialize;

/// Public API error response body.
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    /// Error details.
    pub error: ApiErrorDetail,
}

/// Error details within a public API error response.
#[derive(Debug, Serialize)]
pub struct ApiErrorDetail {
    /// Machine-readable error code.
    pub code: &'static str,
    /// Human-readable error message.
    pub message: String,
}

/// Public API error type that can be converted into an HTTP response.
#[derive(Debug)]
pub struct ApiError {
    /// HTTP status code for the response.
    status: StatusCode,
    /// Machine-readable error code.
    code: &'static str,
    /// Human-readable error message.
    message: String,
}

impl ApiError {
    /// Create a new API error.
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
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

    /// Create a 500 Internal Server Error.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }

    /// Create a 429 Too Many Requests error.
    #[must_use]
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ApiErrorResponse {
            error: ApiErrorDetail {
                code: self.code,
                message: self.message,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message.as_str())
    }
}

impl Error for ApiError {}

/// Result type for public API handlers.
pub type ApiResult<T> = Result<T, ApiError>;

/// Build the mountable Ankh public API router.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/v1/auth/signup", routing::post(auth_routes::signup))
        .route("/api/v1/auth/login", routing::post(auth_routes::login))
        .route("/api/v1/auth/logout", routing::post(auth_routes::logout))
        .route("/api/v1/auth/me", routing::get(auth_routes::me))
        .route(
            "/api/v1/auth/waitlist-status",
            routing::get(auth_routes::waitlist_status),
        )
        .route(
            "/api/v1/auth/verify-email",
            routing::post(auth_routes::verify_email),
        )
        .route(
            "/api/v1/auth/resend-verification",
            routing::post(auth_routes::resend_verification),
        )
        .route(
            "/api/v1/auth/forgot-password",
            routing::post(auth_routes::forgot_password),
        )
        .route(
            "/api/v1/auth/validate-reset-token",
            routing::post(auth_routes::validate_reset_token),
        )
        .route(
            "/api/v1/auth/reset-password",
            routing::post(auth_routes::reset_password),
        )
        .route("/api/v1/orgs", routing::get(org_routes::list_orgs))
        .route("/api/v1/orgs", routing::post(org_routes::create_org))
        .route("/api/v1/orgs/{id}", routing::get(org_routes::get_org))
        .route(
            "/api/v1/orgs/{id}/membership",
            routing::get(org_routes::get_my_membership),
        )
        .route(
            "/api/v1/orgs/{id}/leave",
            routing::post(org_routes::leave_org),
        )
        .route(
            "/api/v1/orgs/{id}/members",
            routing::get(org_routes::list_members),
        )
        .route(
            "/api/v1/orgs/{id}/invites",
            routing::get(org_routes::list_invites),
        )
        .route(
            "/api/v1/orgs/{id}/invites",
            routing::post(org_routes::invite_to_org),
        )
        .route(
            "/api/v1/orgs/{org_id}/members/{member_id}",
            routing::delete(org_routes::remove_member),
        )
        .route(
            "/api/v1/orgs/{org_id}/invites/{invite_id}",
            routing::delete(org_routes::cancel_invite),
        )
        .route(
            "/api/v1/org-invites/{token}",
            routing::get(org_routes::get_invite_details),
        )
        .route(
            "/api/v1/org-invites/{token}/accept",
            routing::post(org_routes::accept_invite),
        )
        .route(
            "/api/v1/device-sessions",
            routing::get(device_routes::list_device_sessions),
        )
        .route(
            "/api/v1/device-sessions",
            routing::post(device_routes::create_device_session),
        )
        .route(
            "/api/v1/device-sessions/{id}",
            routing::delete(device_routes::revoke_device_session),
        )
        .route(
            "/api/v1/device/authorize",
            routing::get(device_routes::authorize),
        )
        .route("/api/v1/device/token", routing::post(device_routes::token))
}
