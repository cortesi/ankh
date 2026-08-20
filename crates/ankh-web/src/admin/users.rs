//! Shared admin user management handlers.

use std::collections::HashMap;

use ankh_constants::USER_INVITE_TTL;
use ankh_db::{Error as DbError, TokenKind, UserId};
use ankh_mail::template;
use ankh_types::admin::{
    InviteAction, InviteUserRequest, InviteUserResponse, ListUsersResponse, ReleaseUserRequest,
    ReleaseUserResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use super::{
    audit::{AdminAuditEvent, AdminAuditResult, RequestContext, emit_admin_audit},
    conversions::{user_detail, user_summary},
    error::{AdminError, AdminResult},
    ids::parse_user_id,
    middleware::SysadminAuth,
    pagination::{clamp_limit, default_limit},
};
use crate::{
    AnkhWebState, DeviceSessionsRevoked, NamespaceDeleted,
    services::auth::{normalize_email, validate_email},
};

/// Query parameters for listing users.
#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    /// Maximum number of users to return.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Pagination cursor for fetching subsequent pages.
    pub cursor: Option<String>,
    /// Filter users by email.
    pub email: Option<String>,
}

/// Internal result for invite operations.
struct InviteOutcome {
    /// Target email address.
    email: String,
    /// Invite action to report.
    action: InviteAction,
    /// Raw invite token, when generated.
    token: Option<String>,
}

/// Target selector for releasing users.
enum ReleaseTarget {
    /// Target by email address.
    Email(String),
    /// Target by user ID.
    Id(UserId),
}

/// List users with optional filtering and pagination.
pub async fn list_users(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Query(query): Query<ListUsersQuery>,
) -> AdminResult<impl IntoResponse> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let (users, next_cursor) = db
        .list_users(
            clamp_limit(query.limit),
            query.cursor.as_deref(),
            query.email.as_deref(),
        )
        .await
        .map_err(|error| AdminError::internal(format!("list users error: {error}")))?;

    Ok((
        StatusCode::OK,
        Json(ListUsersResponse {
            users: users.into_iter().map(user_summary).collect(),
            next_cursor,
        }),
    ))
}

/// Get detailed information about a specific user.
pub async fn get_user(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<impl IntoResponse> {
    let user_id = parse_user_id(id.as_str())?;
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let user = db
        .get_user_by_id(user_id)
        .await
        .map_err(|error| match error {
            DbError::UserMissing(_) => AdminError::not_found("user not found"),
            _ => AdminError::internal(format!("get user error: {error}")),
        })?;

    Ok((StatusCode::OK, Json(user_detail(user))))
}

/// Hard delete a user and revoke their identity sessions.
pub async fn delete_user(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<impl IntoResponse> {
    let user_id = parse_user_id(id.as_str())?;
    let result = delete_user_with_identity_cleanup(&state, user_id).await;
    let audit_result = AdminAuditResult::from(result.is_ok());
    emit_admin_audit(
        &state,
        AdminAuditEvent::new(
            Some(admin.id),
            "user.delete",
            "user",
            id,
            audit_result,
            &ctx,
        ),
    )
    .await;

    result?;
    Ok(StatusCode::NO_CONTENT)
}

/// Delete a user while preserving Ankh-owned revocation semantics.
async fn delete_user_with_identity_cleanup(
    state: &AnkhWebState,
    user_id: UserId,
) -> AdminResult<()> {
    let (revoked, deleted) = {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        let user = db
            .get_user_by_id(user_id)
            .await
            .map_err(|error| match error {
                DbError::UserMissing(_) => AdminError::not_found("user not found"),
                _ => AdminError::internal(format!("get user error: {error}")),
            })?;
        let deleted = NamespaceDeleted {
            namespace_id: user.namespace_id,
            namespace: user.username.clone(),
        };
        let active_device_sessions =
            db.list_device_sessions_for_user(user.id)
                .await
                .map_err(|error| {
                    AdminError::internal(format!("list device sessions error: {error}"))
                })?;
        db.revoke_all_device_sessions(user.id)
            .await
            .map_err(|error| {
                AdminError::internal(format!("revoke device sessions error: {error}"))
            })?;
        db.delete_sessions_for_email(user.email.as_str())
            .await
            .map_err(|error| AdminError::internal(format!("delete sessions error: {error}")))?;
        db.delete_tokens_for_email(user.email.as_str(), TokenKind::PasswordReset)
            .await
            .map_err(|error| AdminError::internal(format!("delete reset tokens error: {error}")))?;
        db.delete_tokens_for_email(user.email.as_str(), TokenKind::EmailVerification)
            .await
            .map_err(|error| {
                AdminError::internal(format!("delete verification tokens error: {error}"))
            })?;
        db.delete_user_by_id(user.id)
            .await
            .map_err(|error| match error {
                DbError::UserMissing(_) => AdminError::not_found("user not found"),
                _ => AdminError::internal(format!("delete user error: {error}")),
            })?;
        (
            DeviceSessionsRevoked {
                user_id: user.id,
                session_ids: active_device_sessions
                    .into_iter()
                    .map(|session| session.id)
                    .collect(),
            },
            deleted,
        )
    };

    if let Err(error) = state.hooks().on_device_sessions_revoked(revoked).await {
        state.record_hook_failure("on_device_sessions_revoked", error);
    }
    if let Err(error) = state.hooks().on_namespaces_deleted(vec![deleted]).await {
        state.record_hook_failure("on_namespaces_deleted", error);
    }
    Ok(())
}

/// Release a waitlisted user and send a notification email.
pub async fn release_user(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<ReleaseUserRequest>,
) -> AdminResult<impl IntoResponse> {
    let target = release_target(request)?;
    let result = release_user_inner(&state, target).await;
    let audit_result = AdminAuditResult::from(result.is_ok());
    let target_id = result
        .as_ref()
        .map(|email| email.as_str())
        .unwrap_or("unknown");
    emit_admin_audit(
        &state,
        AdminAuditEvent::new(
            Some(admin.id),
            "user.release",
            "user",
            target_id,
            audit_result,
            &ctx,
        ),
    )
    .await;

    let email = result?;
    Ok((StatusCode::OK, Json(ReleaseUserResponse { email })))
}

/// Validate and normalize a release target.
fn release_target(request: ReleaseUserRequest) -> AdminResult<ReleaseTarget> {
    match (request.id, request.email) {
        (Some(_), Some(_)) => Err(AdminError::bad_request("provide id or email, not both")),
        (Some(id), None) => Ok(ReleaseTarget::Id(parse_user_id(id.as_str())?)),
        (None, Some(email)) => {
            let email = normalize_email(email.as_str());
            validate_email(email.as_str()).map_err(AdminError::bad_request)?;
            Ok(ReleaseTarget::Email(email))
        }
        (None, None) => Err(AdminError::bad_request("missing id or email")),
    }
}

/// Release a user by target and send release mail.
async fn release_user_inner(state: &AnkhWebState, target: ReleaseTarget) -> AdminResult<String> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let email = match target {
        ReleaseTarget::Email(email) => email,
        ReleaseTarget::Id(user_id) => {
            db.get_user_by_id(user_id)
                .await
                .map_err(|error| match error {
                    DbError::UserMissing(_) => AdminError::not_found("user not found"),
                    _ => AdminError::internal(format!("get user error: {error}")),
                })?
                .email
        }
    };
    db.set_user_waitlisted(email.as_str(), false)
        .await
        .map_err(|error| match error {
            DbError::UserMissing(_) => AdminError::not_found("user not found"),
            _ => AdminError::internal(format!("release user error: {error}")),
        })?;

    let login_url = state.mail().link_url("/login");
    let vars = HashMap::from([("login_url".to_owned(), login_url)]);
    let email_to_send = state
        .mail()
        .render_email(template::WAITLIST_RELEASE, email.as_str(), &vars)
        .map_err(|error| AdminError::internal(format!("mail render error: {error}")))?;
    state
        .mail()
        .send(&email_to_send)
        .await
        .map_err(|error| AdminError::internal(format!("mail send error: {error}")))?;

    Ok(email)
}

/// Invite a user to bypass the waitlist.
pub async fn invite_user(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<InviteUserRequest>,
) -> AdminResult<impl IntoResponse> {
    let email = normalize_email(request.email.as_str());
    validate_email(email.as_str()).map_err(AdminError::bad_request)?;

    let result = invite_user_inner(&state, email).await;
    let audit_result = AdminAuditResult::from(result.is_ok());
    let target_id = result
        .as_ref()
        .map(|outcome| outcome.email.as_str())
        .unwrap_or("unknown");
    emit_admin_audit(
        &state,
        AdminAuditEvent::new(
            Some(admin.id),
            "user.invite",
            "user",
            target_id,
            audit_result,
            &ctx,
        ),
    )
    .await;

    let outcome = result?;
    Ok((
        StatusCode::OK,
        Json(InviteUserResponse {
            email: outcome.email,
            action: outcome.action,
        }),
    ))
}

/// Create or release an invited user and send any required mail.
async fn invite_user_inner(state: &AnkhWebState, email: String) -> AdminResult<InviteOutcome> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let outcome = match db.is_user_waitlisted(email.as_str()).await {
        Ok(waitlisted) => {
            if waitlisted {
                db.set_user_waitlisted(email.as_str(), false)
                    .await
                    .map_err(|error| {
                        AdminError::internal(format!("release user error: {error}"))
                    })?;
                InviteOutcome {
                    email,
                    action: InviteAction::Released,
                    token: None,
                }
            } else {
                InviteOutcome {
                    email,
                    action: InviteAction::AlreadyActive,
                    token: None,
                }
            }
        }
        Err(DbError::UserMissing(_)) => {
            db.delete_invites_for_email(email.as_str())
                .await
                .map_err(|error| AdminError::internal(format!("invite cleanup error: {error}")))?;
            let token = db
                .create_invite(email.as_str(), USER_INVITE_TTL)
                .await
                .map_err(|error| AdminError::internal(format!("invite token error: {error}")))?;
            InviteOutcome {
                email,
                action: InviteAction::Invited,
                token: Some(token),
            }
        }
        Err(error) => {
            return Err(AdminError::internal(format!(
                "waitlist status error: {error}"
            )));
        }
    };

    send_invite_outcome_mail(state, &outcome).await?;
    Ok(outcome)
}

/// Send mail for an invite outcome when one is needed.
async fn send_invite_outcome_mail(
    state: &AnkhWebState,
    outcome: &InviteOutcome,
) -> AdminResult<()> {
    let rendered = match outcome.action {
        InviteAction::Invited => {
            let token = outcome.token.as_deref().expect("invite token");
            let invite_url = format!("{}?invite={token}", state.mail().link_url("/signup"));
            let vars = HashMap::from([("invite_url".to_owned(), invite_url)]);
            Some(state.mail().render_email(
                template::WAITLIST_INVITE,
                outcome.email.as_str(),
                &vars,
            ))
        }
        InviteAction::Released => {
            let login_url = state.mail().link_url("/login");
            let vars = HashMap::from([("login_url".to_owned(), login_url)]);
            Some(state.mail().render_email(
                template::WAITLIST_RELEASE,
                outcome.email.as_str(),
                &vars,
            ))
        }
        InviteAction::AlreadyActive => None,
    };

    if let Some(email) = rendered {
        let email =
            email.map_err(|error| AdminError::internal(format!("mail render error: {error}")))?;
        state
            .mail()
            .send(&email)
            .await
            .map_err(|error| AdminError::internal(format!("mail send error: {error}")))?;
    }
    Ok(())
}
