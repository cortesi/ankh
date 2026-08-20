//! Device-session services.

use std::{num::NonZeroU32, sync::OnceLock};

use ankh_constants::{
    DEVICE_AUTH_EXCHANGE_RATE_PER_MINUTE, DEVICE_AUTH_GRANT_TTL, DEVICE_NAME_MAX_LEN,
    DEVICE_SESSION_TTL,
};
use ankh_db::{DeviceAuthGrantRequest, DeviceSession, Error as DbError, Session};
use ankh_types::{
    CreateDeviceSessionResponse, DeviceAuthorizationRequest, DeviceAuthorizationResponse,
    DevicePlatform, DeviceSessionId, DeviceSessionInfo, DeviceTokenRequest, DeviceTokenResponse,
};
use axum::http::HeaderMap;
use chrono::Utc;
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use url::Url;

use crate::{
    api::{ApiError, ApiResult},
    auth::bad_request,
    errors,
    hooks::DeviceSessionsRevoked,
    state::AnkhWebState,
};

/// Device-session name assigned to browser-backed sessions. Used both to
/// recognize a user's existing browser session (so it can be revoked) and as
/// the name of the replacement; the two must stay identical.
const BROWSER_DEVICE_NAME: &str = "Browser Player";

/// Shared rate limiters for device authorization flows.
struct DeviceAuthRateLimiters {
    /// Rate limit auth exchanges per IP.
    exchange_by_ip: DefaultKeyedRateLimiter<String>,
}

/// Fetch or initialize the device auth rate limiters.
fn device_auth_rate_limiters() -> &'static DeviceAuthRateLimiters {
    static LIMITERS: OnceLock<DeviceAuthRateLimiters> = OnceLock::new();
    LIMITERS.get_or_init(|| DeviceAuthRateLimiters {
        exchange_by_ip: RateLimiter::keyed(Quota::per_minute(nonzero(
            DEVICE_AUTH_EXCHANGE_RATE_PER_MINUTE,
        ))),
    })
}

/// Convert a u32 to a non-zero value.
fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("non-zero rate limit")
}

/// Extract the client IP from proxy headers.
fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Enforce the auth exchange rate limit for an IP.
fn enforce_exchange_rate_limit(ip: &str) -> ApiResult<()> {
    let limiters = device_auth_rate_limiters();
    if limiters.exchange_by_ip.check_key(&ip.to_string()).is_err() {
        return Err(ApiError::too_many_requests("too many auth attempts"));
    }
    Ok(())
}

/// Derive the display status for a device session.
fn session_status(session: &DeviceSession) -> &'static str {
    let now = Utc::now();
    if session.revoked_at.is_some() {
        "revoked"
    } else if session.expires_at <= now {
        "expired"
    } else {
        "active"
    }
}

/// Convert a DB device session into the public list DTO.
fn session_info(session: DeviceSession) -> DeviceSessionInfo {
    let status = session_status(&session).to_owned();
    DeviceSessionInfo {
        id: session.id.to_string(),
        device_name: session.device_name,
        platform: session.platform,
        status,
        created_at: session.created_at.to_rfc3339(),
        last_used_at: session.last_used_at.to_rfc3339(),
        expires_at: session.expires_at.to_rfc3339(),
    }
}

/// Resolve the current database user from a web session email.
async fn load_user_for_session(
    state: &AnkhWebState,
    session: &Session,
) -> ApiResult<ankh_db::UserDetail> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    db.get_user_by_email(session.email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))
}

/// List active device sessions for the current user.
pub async fn list_device_sessions(
    state: &AnkhWebState,
    session: &Session,
) -> ApiResult<Vec<DeviceSessionInfo>> {
    let user = load_user_for_session(state, session).await?;
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let sessions = db
        .list_device_sessions_for_user(user.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    Ok(sessions.into_iter().map(session_info).collect())
}

/// Revoke a device session owned by the current user.
pub async fn revoke_device_session(
    state: &AnkhWebState,
    session: &Session,
    id: String,
) -> ApiResult<()> {
    let session_id: DeviceSessionId = id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_DEVICE_SESSION_ID))?;
    let user = load_user_for_session(state, session).await?;
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    db.revoke_device_session(session_id, user.id)
        .await
        .map_err(|err| match err {
            DbError::DeviceSessionMissing(_) => ApiError::not_found("session not found"),
            _ => ApiError::internal(err.to_string()),
        })?;
    dispatch_device_sessions_revoked(
        state,
        DeviceSessionsRevoked {
            user_id: user.id,
            session_ids: vec![session_id],
        },
    )
    .await;
    Ok(())
}

/// Mint a browser-hosted device session for the current web session.
pub async fn create_browser_device_session(
    state: &AnkhWebState,
    session: &Session,
) -> ApiResult<CreateDeviceSessionResponse> {
    let user = load_user_for_session(state, session).await?;
    let (created, revoked_ids) = {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let now = Utc::now();
        let existing = db
            .list_device_sessions_for_user(user.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let mut revoked_ids = Vec::new();
        for session in existing {
            if session.device_name == BROWSER_DEVICE_NAME
                && session.revoked_at.is_none()
                && session.expires_at > now
            {
                match db.revoke_device_session(session.id, user.id).await {
                    Ok(()) | Err(DbError::DeviceSessionMissing(_)) => {
                        revoked_ids.push(session.id);
                    }
                    Err(err) => return Err(ApiError::internal(err.to_string())),
                }
            }
        }

        let created = db
            .create_device_session(
                user.id,
                BROWSER_DEVICE_NAME,
                &DevicePlatform::Web,
                DEVICE_SESSION_TTL,
            )
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        (created, revoked_ids)
    };

    dispatch_device_sessions_revoked(
        state,
        DeviceSessionsRevoked {
            user_id: user.id,
            session_ids: revoked_ids,
        },
    )
    .await;
    Ok(CreateDeviceSessionResponse {
        token: created.token,
        device_name: created.session.device_name,
        expires_at: created.session.expires_at.to_rfc3339(),
    })
}

/// Create a device authorization grant for an authenticated browser session.
pub async fn authorize_device(
    state: &AnkhWebState,
    session: &Session,
    request: DeviceAuthorizationRequest,
) -> ApiResult<DeviceAuthorizationResponse> {
    validate_authorization_request(&request)?;
    let user = load_user_for_session(state, session).await?;
    let redirect_port = i32::from(request.redirect_port);
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let created = db
        .create_device_auth_grant(DeviceAuthGrantRequest {
            user_id: user.id,
            code_challenge: request.code_challenge.trim(),
            state: request.state.trim(),
            redirect_port,
            device_name: request.device_name.trim(),
            platform: request.platform.clone(),
            ttl: DEVICE_AUTH_GRANT_TTL,
        })
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let callback_url =
        build_callback_redirect_url(redirect_port, created.code.as_str(), request.state.trim())?;

    Ok(DeviceAuthorizationResponse {
        code: created.code,
        callback_url,
        expires_at: Utc::now()
            + chrono::Duration::from_std(DEVICE_AUTH_GRANT_TTL)
                .expect("device grant ttl fits chrono"),
    })
}

/// Exchange a device authorization code for a bearer session.
pub async fn exchange_device_token(
    state: &AnkhWebState,
    headers: &HeaderMap,
    request: DeviceTokenRequest,
) -> ApiResult<DeviceTokenResponse> {
    let ip = extract_client_ip(headers);
    enforce_exchange_rate_limit(ip.as_str())?;

    let mut db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let grant = db
        .consume_device_auth_grant(request.code.as_str(), request.code_verifier.as_str())
        .await
        .map_err(|err| match err {
            DbError::DeviceAuthGrantMissing(_)
            | DbError::DeviceAuthGrantExpired(_)
            | DbError::DeviceAuthGrantConsumed(_)
            | DbError::DeviceAuthGrantAttemptsExceeded(_) => {
                ApiError::bad_request("invalid or expired code")
            }
            DbError::DeviceAuthGrantInvalidVerifier(_) => {
                ApiError::unauthorized("invalid code verifier")
            }
            _ => ApiError::internal(err.to_string()),
        })?;

    let created = db
        .create_device_session(
            grant.user_id,
            grant.device_name.as_str(),
            &grant.platform,
            DEVICE_SESSION_TTL,
        )
        .await
        .map_err(|err| match err {
            DbError::DeviceSessionLimitReached(_) => {
                ApiError::bad_request("device session limit reached")
            }
            _ => ApiError::internal(err.to_string()),
        })?;

    Ok(DeviceTokenResponse {
        token: created.token,
        device_name: created.session.device_name,
        platform: created.session.platform,
        expires_at: created.session.expires_at,
    })
}

/// Validate and normalize a device authorization request.
fn validate_authorization_request(request: &DeviceAuthorizationRequest) -> ApiResult<()> {
    if request.code_challenge.trim().is_empty() {
        return Err(ApiError::bad_request("missing code_challenge"));
    }
    if request.state.trim().is_empty() {
        return Err(ApiError::bad_request("missing state"));
    }
    let device_name = request.device_name.trim();
    if device_name.is_empty() {
        return Err(ApiError::bad_request("missing device_name"));
    }
    if device_name.len() > DEVICE_NAME_MAX_LEN {
        return Err(ApiError::bad_request("device_name too long"));
    }
    Ok(())
}

/// Build a loopback callback URL with encoded query values.
fn build_callback_redirect_url(redirect_port: i32, code: &str, state: &str) -> ApiResult<String> {
    let base = format!("http://127.0.0.1:{redirect_port}/callback");
    let mut url = Url::parse(base.as_str()).map_err(|err| ApiError::internal(err.to_string()))?;
    url.query_pairs_mut()
        .append_pair("code", code)
        .append_pair("state", state);
    Ok(url.to_string())
}

/// Dispatch device-session revocation hooks best-effort.
async fn dispatch_device_sessions_revoked(state: &AnkhWebState, payload: DeviceSessionsRevoked) {
    if payload.session_ids.is_empty() {
        return;
    }
    if let Err(error) = state.hooks().on_device_sessions_revoked(payload).await {
        state.record_hook_failure("on_device_sessions_revoked", error);
    }
}
