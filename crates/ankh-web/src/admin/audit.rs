//! Shared admin audit events and request context extraction.

use std::{convert::Infallible, net::IpAddr};

use ankh_db::SysadminId;
use async_trait::async_trait;
use axum::{
    extract::FromRequestParts,
    http::{header::USER_AGENT, request::Parts},
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Result label recorded for an admin operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminAuditResult {
    /// Operation succeeded.
    Success,
    /// Operation failed.
    Failure,
}

impl AdminAuditResult {
    /// Return the stable text value for this result.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}

impl From<bool> for AdminAuditResult {
    /// Map a success flag to the corresponding result label.
    fn from(succeeded: bool) -> Self {
        if succeeded {
            Self::Success
        } else {
            Self::Failure
        }
    }
}

/// Audit event representing an admin operation.
#[derive(Clone, Debug)]
pub struct AdminAuditEvent {
    /// Admin ID that performed the action, when known.
    pub sysadmin_id: Option<SysadminId>,
    /// Action performed, for example `user.delete`.
    pub action: &'static str,
    /// Type of target entity, for example `user`.
    pub target_type: &'static str,
    /// ID or stable selector for the target entity.
    pub target_id: String,
    /// Request ID for correlation.
    pub request_id: String,
    /// Client IP address.
    pub ip: Option<IpAddr>,
    /// Client user agent.
    pub user_agent: Option<String>,
    /// When the event occurred.
    pub occurred_at: DateTime<Utc>,
    /// Result of the operation.
    pub result: AdminAuditResult,
}

impl AdminAuditEvent {
    /// Build an audit event from request context.
    #[must_use]
    pub fn new(
        sysadmin_id: Option<SysadminId>,
        action: &'static str,
        target_type: &'static str,
        target_id: impl Into<String>,
        result: AdminAuditResult,
        ctx: &RequestContext,
    ) -> Self {
        Self {
            sysadmin_id,
            action,
            target_type,
            target_id: target_id.into(),
            request_id: ctx.request_id.clone(),
            ip: ctx.ip,
            user_agent: ctx.user_agent.clone(),
            occurred_at: Utc::now(),
            result,
        }
    }
}

/// Sink for admin audit events.
#[async_trait]
pub trait AdminAudit: Send + Sync {
    /// Record an admin audit event.
    async fn record(&self, event: AdminAuditEvent) -> Result<(), String>;
}

/// Audit sink that emits events through tracing.
#[derive(Clone, Debug, Default)]
pub struct TracingAdminAudit;

#[async_trait]
impl AdminAudit for TracingAdminAudit {
    async fn record(&self, event: AdminAuditEvent) -> Result<(), String> {
        tracing::info!(
            sysadmin_id = event.sysadmin_id.map(|id| id.to_string()),
            action = event.action,
            target_type = event.target_type,
            target_id = event.target_id,
            request_id = event.request_id,
            ip = ?event.ip,
            user_agent = event.user_agent,
            occurred_at = %event.occurred_at.to_rfc3339(),
            result = event.result.as_str(),
            "admin_audit_event"
        );
        Ok(())
    }
}

/// Request context extractor for audit logging.
pub struct RequestContext {
    /// Request ID for correlation.
    pub request_id: String,
    /// Client IP address.
    pub ip: Option<IpAddr>,
    /// Client user agent.
    pub user_agent: Option<String>,
}

impl<S> FromRequestParts<S> for RequestContext
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let request_id = parts
            .headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let ip = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .and_then(|value| value.trim().parse().ok())
            .or_else(|| {
                parts
                    .headers
                    .get("x-real-ip")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse().ok())
            });

        let user_agent = parts
            .headers
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        Ok(Self {
            request_id,
            ip,
            user_agent,
        })
    }
}

/// Emit an audit event and record best-effort failures on shared state.
pub async fn emit_admin_audit(state: &crate::AnkhWebState, event: AdminAuditEvent) {
    if let Err(error) = state.admin_audit().record(event).await {
        state.record_audit_failure("admin_audit", error);
    }
}
