//! Server-side authentication helpers and extractors.

use std::{
    num::NonZeroU32,
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ankh_constants::{
    PASSWORD_RESET_GLOBAL_PER_HOUR, PASSWORD_RESET_RATE_PER_HOUR, SESSION_TOUCH_STALE_AFTER,
    SIGNUP_GLOBAL_PER_HOUR, SIGNUP_RATE_PER_HOUR, USER_LOGIN_GLOBAL_PER_MINUTE,
    USER_LOGIN_RATE_PER_MINUTE,
};
use ankh_db::{DeviceSession, Error as DbError, Session};
use axum::{
    extract::FromRequestParts,
    http::{
        HeaderMap, HeaderValue,
        header::{AUTHORIZATION, COOKIE, HeaderName, SET_COOKIE},
        request::Parts,
    },
};
use governor::{DefaultDirectRateLimiter, DefaultKeyedRateLimiter, Quota, RateLimiter};

use crate::{
    api::ApiError,
    errors,
    state::{AnkhWebState, CookieConfig},
};

/// Shared auth rate limiters keyed by action and subject.
struct AuthRateLimiters {
    /// Per-email login limiter.
    login_by_email: DefaultKeyedRateLimiter<String>,
    /// Global login limiter.
    login_global: DefaultDirectRateLimiter,
    /// Per-email signup limiter.
    signup_by_email: DefaultKeyedRateLimiter<String>,
    /// Global signup limiter.
    signup_global: DefaultDirectRateLimiter,
    /// Per-email reset limiter.
    reset_by_email: DefaultKeyedRateLimiter<String>,
    /// Global reset limiter.
    reset_global: DefaultDirectRateLimiter,
}

impl AuthRateLimiters {
    /// Build the shared auth limiter set.
    fn new() -> Self {
        Self {
            login_by_email: RateLimiter::keyed(Quota::per_minute(nonzero(
                USER_LOGIN_RATE_PER_MINUTE,
            ))),
            login_global: RateLimiter::direct(Quota::per_minute(nonzero(
                USER_LOGIN_GLOBAL_PER_MINUTE,
            ))),
            signup_by_email: RateLimiter::keyed(Quota::per_hour(nonzero(SIGNUP_RATE_PER_HOUR))),
            signup_global: RateLimiter::direct(Quota::per_hour(nonzero(SIGNUP_GLOBAL_PER_HOUR))),
            reset_by_email: RateLimiter::keyed(Quota::per_hour(nonzero(
                PASSWORD_RESET_RATE_PER_HOUR,
            ))),
            reset_global: RateLimiter::direct(Quota::per_hour(nonzero(
                PASSWORD_RESET_GLOBAL_PER_HOUR,
            ))),
        }
    }
}

/// Return the lazily initialized auth limiter set.
fn auth_rate_limiters() -> &'static AuthRateLimiters {
    static LIMITERS: OnceLock<AuthRateLimiters> = OnceLock::new();
    LIMITERS.get_or_init(AuthRateLimiters::new)
}

/// Convert a positive integer into `NonZeroU32`.
fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("non-zero rate limit")
}

/// Apply a global limiter and a per-email limiter, returning the shared
/// rate-limited error if either rejects the request.
fn enforce_rate_limit(
    global: &DefaultDirectRateLimiter,
    by_email: &DefaultKeyedRateLimiter<String>,
    email: &str,
) -> Result<(), ApiError> {
    if global.check().is_err() {
        return Err(rate_limited());
    }
    if by_email.check_key(&email.to_string()).is_err() {
        return Err(rate_limited());
    }
    Ok(())
}

/// Apply global and per-email login rate limits.
pub fn enforce_login_rate_limit(email: &str) -> Result<(), ApiError> {
    let limiters = auth_rate_limiters();
    enforce_rate_limit(&limiters.login_global, &limiters.login_by_email, email)
}

/// Apply global and per-email signup rate limits.
pub fn enforce_signup_rate_limit(email: &str) -> Result<(), ApiError> {
    let limiters = auth_rate_limiters();
    enforce_rate_limit(&limiters.signup_global, &limiters.signup_by_email, email)
}

/// Apply global and per-email password reset rate limits.
pub fn enforce_password_reset_rate_limit(email: &str) -> Result<(), ApiError> {
    let limiters = auth_rate_limiters();
    enforce_rate_limit(&limiters.reset_global, &limiters.reset_by_email, email)
}

/// Current unix epoch timestamp in seconds.
#[must_use]
pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

/// Create a bad-request error with a message.
#[must_use]
pub fn bad_request(message: &'static str) -> ApiError {
    ApiError::bad_request(message)
}

/// Create an unauthorized error with a message.
#[must_use]
pub fn unauthorized(message: &'static str) -> ApiError {
    ApiError::unauthorized(message)
}

/// Create a too-many-requests error with a message.
#[must_use]
pub fn too_many_requests(message: &'static str) -> ApiError {
    ApiError::too_many_requests(message)
}

/// Create a rate-limited error for auth endpoints.
#[must_use]
pub fn rate_limited() -> ApiError {
    too_many_requests(errors::RATE_LIMITED)
}

/// Build a session `Set-Cookie` header.
#[must_use]
pub fn session_set_cookie_header(session_token: &str, config: &CookieConfig) -> HeaderValue {
    session_cookie_header(session_token, config, ankh_constants::DEFAULT_SESSION_TTL)
}

/// Build a clearing `Set-Cookie` header.
#[must_use]
pub fn session_clear_cookie_header(config: &CookieConfig) -> HeaderValue {
    session_cookie_header("", config, Duration::from_secs(0))
}

/// Extract the current session token from request headers.
#[must_use]
pub fn current_session_id(headers: &HeaderMap, config: &CookieConfig) -> Option<String> {
    headers
        .get(COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie| session_id_from_cookie(cookie, config.session_cookie_name.as_str()))
}

/// Extract a bearer token from request headers.
pub fn bearer_token(headers: &HeaderMap) -> Result<String, ApiError> {
    let auth = headers
        .get(AUTHORIZATION)
        .ok_or_else(|| ApiError::unauthorized("Missing authorization header"))?
        .to_str()
        .map_err(|_| ApiError::unauthorized("Invalid authorization header"))?;

    let token = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))
        .ok_or_else(|| ApiError::unauthorized("Invalid authorization header format"))?;

    if token.is_empty() {
        return Err(ApiError::unauthorized("Empty bearer token"));
    }

    Ok(token.to_string())
}

/// Build the raw `Set-Cookie` header value for a session token.
fn session_cookie_header(
    session_token: &str,
    config: &CookieConfig,
    max_age: Duration,
) -> HeaderValue {
    let max_age_seconds: i64 = max_age.as_secs().try_into().unwrap_or(i64::MAX);
    let secure = if config.secure { "; Secure" } else { "" };

    let value = format!(
        "{}={session_token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}{secure}",
        config.session_cookie_name
    );
    HeaderValue::from_str(value.as_str()).expect("valid session Set-Cookie header")
}

/// Extract the session ID from a raw `Cookie` header value.
fn session_id_from_cookie(cookie: &str, session_cookie_name: &str) -> Option<String> {
    cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix(session_cookie_name)
            .and_then(|suffix| suffix.strip_prefix('='))
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty())
    })
}

/// Extracted auth session, if available.
pub struct AuthSession(
    /// Session data when authenticated.
    pub Option<Session>,
);

impl<S> FromRequestParts<S> for AuthSession
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(state) = parts.extensions.get::<AnkhWebState>().cloned() else {
            return Err(ApiError::internal("missing Ankh web state"));
        };
        let Some(session_id) = current_session_id(&parts.headers, &state.config().cookie) else {
            return Ok(Self(None));
        };

        let mut db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let session = match db
            .touch_session_if_stale(session_id.as_str(), SESSION_TOUCH_STALE_AFTER)
            .await
        {
            Ok(session) => Some(session),
            Err(DbError::SessionMissing(_)) => None,
            Err(err) => return Err(ApiError::internal(err.to_string())),
        };

        Ok(Self(session))
    }
}

/// Extractor that requires a valid web session.
pub struct RequireSession(
    /// Session data when authenticated.
    pub Session,
);

impl<S> FromRequestParts<S> for RequireSession
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AuthSession(session) = AuthSession::from_request_parts(parts, state).await?;
        let session = session.ok_or_else(|| unauthorized(errors::UNAUTHORIZED))?;
        Ok(Self(session))
    }
}

/// Extractor that requires a valid web session for a non-waitlisted user.
///
/// Resolves the session like [`RequireSession`], then rejects with 403 when the
/// account is waitlisted. Use this on product routes that a waitlisted user must
/// not reach; use [`RequireSession`] for routes a waitlisted user still needs
/// (e.g. `me`, `logout`, waitlist status).
pub struct RequireActiveUser(
    /// Session data for the authenticated, non-waitlisted user.
    pub Session,
);

impl<S> FromRequestParts<S> for RequireActiveUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Some(web_state) = parts.extensions.get::<AnkhWebState>().cloned() else {
            return Err(ApiError::internal("missing Ankh web state"));
        };
        let RequireSession(session) = RequireSession::from_request_parts(parts, state).await?;

        let db = web_state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let waitlisted = db
            .is_user_waitlisted(session.email.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        if waitlisted {
            return Err(ApiError::forbidden(errors::WAITLISTED));
        }

        Ok(Self(session))
    }
}

/// Extractor that requires a valid device bearer session.
pub struct DeviceBearerSession(
    /// Validated device session.
    pub DeviceSession,
);

impl<S> FromRequestParts<S> for DeviceBearerSession
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(state) = parts.extensions.get::<AnkhWebState>().cloned() else {
            return Err(ApiError::internal("missing Ankh web state"));
        };
        let token = bearer_token(&parts.headers)?;
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let session =
            db.validate_device_session(token.as_str())
                .await
                .map_err(|err| match err {
                    DbError::DeviceSessionMissing(_)
                    | DbError::DeviceSessionExpired(_)
                    | DbError::DeviceSessionRevoked(_) => ApiError::unauthorized("Unauthorized"),
                    _ => ApiError::internal(err.to_string()),
                })?;
        Ok(Self(session))
    }
}

/// Header key for a `Set-Cookie` response.
pub const SET_COOKIE_HEADER: HeaderName = SET_COOKIE;
