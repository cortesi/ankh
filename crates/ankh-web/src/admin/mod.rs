//! Shared admin API errors, extractors, audit sinks, handlers, and router.

mod audit;
mod auth;
mod conversions;
mod device_sessions;
mod error;
mod ids;
mod middleware;
mod namespaces;
mod orgs;
mod pagination;
mod router;
mod sessions;
mod settings;
mod sysadmins;
mod users;

pub use audit::{
    AdminAudit, AdminAuditEvent, AdminAuditResult, RequestContext, TracingAdminAudit,
    emit_admin_audit,
};
pub use error::{AdminError, AdminResult};
pub use middleware::SysadminAuth;
pub use router::admin_router;
