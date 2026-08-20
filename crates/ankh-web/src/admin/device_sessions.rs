//! Shared device session administration handlers.

use ankh_db::{DeviceSessionStatus, Error as DbError, UserId};
use ankh_types::admin::ListDeviceSessionsResponse;
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use serde::Deserialize;

use super::{
    audit::{AdminAuditEvent, AdminAuditResult, RequestContext, emit_admin_audit},
    conversions::device_session_summary_at,
    error::{AdminError, AdminResult},
    ids::{parse_device_session_id, parse_user_id},
    middleware::SysadminAuth,
    pagination::{clamp_limit, default_limit},
};
use crate::{AnkhWebState, DeviceSessionsRevoked};

/// Query parameters for listing device sessions.
#[derive(Debug, Deserialize)]
pub struct ListDeviceSessionsQuery {
    /// Maximum number of sessions to return.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Pagination cursor for fetching subsequent pages.
    pub cursor: Option<String>,
    /// Filter sessions by user ID.
    pub user_id: Option<String>,
    /// Filter sessions by status.
    pub status: Option<String>,
}

/// Parse a status filter into a device session status.
fn parse_status(status: &str) -> Option<DeviceSessionStatus> {
    match status.to_lowercase().as_str() {
        "active" => Some(DeviceSessionStatus::Active),
        "revoked" => Some(DeviceSessionStatus::Revoked),
        "expired" => Some(DeviceSessionStatus::Expired),
        _ => None,
    }
}

/// List device sessions with optional filtering and pagination.
pub async fn list_device_sessions(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Query(query): Query<ListDeviceSessionsQuery>,
) -> AdminResult<impl IntoResponse> {
    let user_id: Option<UserId> = query.user_id.map(|id| parse_user_id(&id)).transpose()?;
    let status = query
        .status
        .map(|status| {
            parse_status(status.as_str())
                .ok_or_else(|| AdminError::bad_request("invalid status value"))
        })
        .transpose()?;

    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let (sessions, next_cursor) = db
        .list_device_sessions(
            clamp_limit(query.limit),
            query.cursor.as_deref(),
            user_id,
            status,
        )
        .await
        .map_err(|error| AdminError::internal(format!("list device sessions error: {error}")))?;

    let now = Utc::now();
    Ok((
        StatusCode::OK,
        Json(ListDeviceSessionsResponse {
            sessions: sessions
                .into_iter()
                .map(|summary| device_session_summary_at(summary, now))
                .collect(),
            next_cursor,
        }),
    ))
}

/// Revoke an active device session.
pub async fn revoke_device_session(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<impl IntoResponse> {
    let session_id = parse_device_session_id(id.as_str())?;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        let session = db
            .get_device_session(session_id)
            .await
            .map_err(|error| match error {
                DbError::DeviceSessionMissing(_) => {
                    AdminError::not_found("device session not found")
                }
                _ => AdminError::internal(format!("get device session error: {error}")),
            })?;
        db.revoke_device_session_by_id(session_id)
            .await
            .map_err(|error| match error {
                DbError::DeviceSessionMissing(_) => {
                    AdminError::not_found("device session not found")
                }
                _ => AdminError::internal(format!("revoke device session error: {error}")),
            })?;
        Ok(session.user_id)
    }
    .await;

    let audit_result = AdminAuditResult::from(result.is_ok());
    emit_admin_audit(
        &state,
        AdminAuditEvent::new(
            Some(admin.id),
            "device_session.revoke",
            "device_session",
            id,
            audit_result,
            &ctx,
        ),
    )
    .await;

    let user_id = result?;
    if let Err(error) = state
        .hooks()
        .on_device_sessions_revoked(DeviceSessionsRevoked {
            user_id,
            session_ids: vec![session_id],
        })
        .await
    {
        state.record_hook_failure("on_device_sessions_revoked", error);
    }
    Ok(StatusCode::NO_CONTENT)
}
