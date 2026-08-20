//! Shared web session administration handlers.

use ankh_db::{Error as DbError, SessionStatus, UserId};
use ankh_types::admin::ListSessionsResponse;
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
    conversions::session_summary_at,
    error::{AdminError, AdminResult},
    ids::{parse_session_id, parse_user_id},
    middleware::SysadminAuth,
    pagination::{clamp_limit, default_limit},
};
use crate::AnkhWebState;

/// Query parameters for listing sessions.
#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
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

/// Parse a status string into a session status value.
fn parse_status(status: &str) -> Option<SessionStatus> {
    match status.to_lowercase().as_str() {
        "active" => Some(SessionStatus::Active),
        "revoked" => Some(SessionStatus::Revoked),
        "expired" => Some(SessionStatus::Expired),
        _ => None,
    }
}

/// List web sessions with optional filtering and pagination.
pub async fn list_sessions(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Query(query): Query<ListSessionsQuery>,
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
        .list_sessions(
            clamp_limit(query.limit),
            query.cursor.as_deref(),
            user_id,
            status,
        )
        .await
        .map_err(|error| AdminError::internal(format!("list sessions error: {error}")))?;

    let now = Utc::now();
    Ok((
        StatusCode::OK,
        Json(ListSessionsResponse {
            sessions: sessions
                .into_iter()
                .map(|summary| session_summary_at(summary, now))
                .collect(),
            next_cursor,
        }),
    ))
}

/// Revoke an active web session.
pub async fn revoke_session(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<impl IntoResponse> {
    let session_id = parse_session_id(id.as_str())?;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.revoke_session_by_id(session_id)
            .await
            .map_err(|error| match error {
                DbError::SessionMissing(_) => AdminError::not_found("session not found"),
                _ => AdminError::internal(format!("revoke session error: {error}")),
            })
    }
    .await;

    let audit_result = AdminAuditResult::from(result.is_ok());
    emit_admin_audit(
        &state,
        AdminAuditEvent::new(
            Some(admin.id),
            "session.revoke",
            "session",
            id,
            audit_result,
            &ctx,
        ),
    )
    .await;

    result?;
    Ok(StatusCode::NO_CONTENT)
}
