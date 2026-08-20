//! Shared sysadmin authentication handlers.

use std::{num::NonZeroU32, sync::OnceLock};

use ankh_constants::{ADMIN_LOGIN_GLOBAL_RATE_PER_MINUTE, DEFAULT_SYSADMIN_TOKEN_TTL};
use ankh_db::Error as DbError;
use ankh_types::admin::{AdminLoginRequest, LoginResponse};
use axum::{Extension, Json, http::StatusCode, response::IntoResponse};
use chrono::Utc;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};

use super::{
    audit::{AdminAuditEvent, AdminAuditResult, RequestContext, emit_admin_audit},
    conversions::sysadmin_identity,
    error::{AdminError, AdminResult},
};
use crate::AnkhWebState;

/// Rate limiter for sysadmin login attempts.
fn login_rate_limiter() -> &'static DefaultDirectRateLimiter {
    static LIMITER: OnceLock<DefaultDirectRateLimiter> = OnceLock::new();
    LIMITER.get_or_init(|| {
        RateLimiter::direct(Quota::per_minute(
            NonZeroU32::new(ADMIN_LOGIN_GLOBAL_RATE_PER_MINUTE).expect("nonzero rate"),
        ))
    })
}

/// Authenticate a sysadmin and return a bearer token.
pub async fn login(
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<AdminLoginRequest>,
) -> AdminResult<impl IntoResponse> {
    if login_rate_limiter().check().is_err() {
        return Err(AdminError::rate_limited("too many login attempts"));
    }
    if request.email.is_empty() {
        return Err(AdminError::bad_request("email is required"));
    }
    if request.password.is_empty() {
        return Err(AdminError::bad_request("password is required"));
    }

    let email = request.email.trim().to_lowercase();
    let password = request.password;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.sysadmin_login(
            email.as_str(),
            password.as_str(),
            DEFAULT_SYSADMIN_TOKEN_TTL,
        )
        .await
        .map_err(|error| match error {
            DbError::InvalidCredentials => AdminError::unauthorized("invalid credentials"),
            DbError::SysadminDisabled(email) => {
                AdminError::forbidden(format!("sysadmin account disabled: {email}"))
            }
            _ => AdminError::internal(format!("login error: {error}")),
        })
    }
    .await;

    let audit = match &result {
        Ok((_, info)) => AdminAuditEvent::new(
            Some(info.id),
            "admin.login",
            "sysadmin",
            email.as_str(),
            AdminAuditResult::Success,
            &ctx,
        ),
        Err(_) => AdminAuditEvent::new(
            None,
            "admin.login",
            "sysadmin",
            email.as_str(),
            AdminAuditResult::Failure,
            &ctx,
        ),
    };
    emit_admin_audit(&state, audit).await;

    let (token, sysadmin_info) = result?;
    let expires_at = Utc::now()
        + chrono::Duration::from_std(DEFAULT_SYSADMIN_TOKEN_TTL)
            .expect("sysadmin ttl fits chrono duration");
    let response = LoginResponse {
        token,
        expires_at,
        sysadmin: sysadmin_identity(sysadmin_info),
    };

    Ok((StatusCode::OK, Json(response)))
}
