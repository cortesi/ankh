//! Shared admin namespace suspension handlers.

use ankh_db::Error as DbError;
use ankh_types::admin::NamespaceStatusResponse;
use axum::{Extension, Json, extract::Path, http::StatusCode};

use super::{
    audit::{AdminAuditEvent, AdminAuditResult, RequestContext, emit_admin_audit},
    error::{AdminError, AdminResult},
    ids::parse_namespace_id,
    middleware::SysadminAuth,
};
use crate::{AnkhWebState, NamespaceStatusChanged};

/// Suspend a namespace and dispatch product cleanup hooks.
pub async fn suspend_namespace(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<(StatusCode, Json<NamespaceStatusResponse>)> {
    set_namespace_suspension(&state, admin.id, &ctx, id, true).await
}

/// Reinstate a suspended namespace and dispatch product cleanup hooks.
pub async fn reinstate_namespace(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<(StatusCode, Json<NamespaceStatusResponse>)> {
    set_namespace_suspension(&state, admin.id, &ctx, id, false).await
}

/// Set namespace suspension state, audit the mutation, and dispatch hooks.
async fn set_namespace_suspension(
    state: &AnkhWebState,
    admin_id: ankh_db::SysadminId,
    ctx: &RequestContext,
    id: String,
    suspended: bool,
) -> AdminResult<(StatusCode, Json<NamespaceStatusResponse>)> {
    let namespace_id = parse_namespace_id(id.as_str())?;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.set_namespace_suspended(namespace_id, suspended)
            .await
            .map_err(|error| match error {
                DbError::NamespaceMissing(_) => AdminError::not_found("namespace not found"),
                _ => AdminError::internal(format!("namespace status error: {error}")),
            })
    }
    .await;

    let action = if suspended {
        "namespace.suspend"
    } else {
        "namespace.reinstate"
    };
    let audit_result = AdminAuditResult::from(result.is_ok());
    emit_admin_audit(
        state,
        AdminAuditEvent::new(Some(admin_id), action, "namespace", id, audit_result, ctx),
    )
    .await;

    let update = result?;
    let payload = NamespaceStatusChanged {
        namespace_id,
        namespace: update.name.clone(),
        suspended: update.suspended,
        r#gen: update.r#gen,
    };
    let hook_result = if suspended {
        state.hooks().on_namespace_suspended(payload).await
    } else {
        state.hooks().on_namespace_reinstated(payload).await
    };
    if let Err(error) = hook_result {
        let hook = if suspended {
            "on_namespace_suspended"
        } else {
            "on_namespace_reinstated"
        };
        state.record_hook_failure(hook, error);
    }

    Ok((
        StatusCode::OK,
        Json(NamespaceStatusResponse {
            id: namespace_id.to_string(),
            status: if update.suspended {
                "suspended".to_owned()
            } else {
                "active".to_owned()
            },
            r#gen: update.r#gen,
        }),
    ))
}
