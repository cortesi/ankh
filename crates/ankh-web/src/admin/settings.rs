//! Shared admin settings handlers.

use ankh_types::admin::WaitlistSettingsRequest;
use axum::{Extension, Json, http::StatusCode, response::IntoResponse};

use super::{
    audit::{AdminAuditEvent, AdminAuditResult, RequestContext, emit_admin_audit},
    conversions::settings_response,
    error::{AdminError, AdminResult},
    middleware::SysadminAuth,
};
use crate::AnkhWebState;

/// Fetch global identity settings.
pub async fn get_settings(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
) -> AdminResult<impl IntoResponse> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let settings = db
        .get_app_settings()
        .await
        .map_err(|error| AdminError::internal(format!("settings error: {error}")))?;

    Ok((StatusCode::OK, Json(settings_response(settings))))
}

/// Enable or disable waitlist mode.
pub async fn set_waitlist(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<WaitlistSettingsRequest>,
) -> AdminResult<impl IntoResponse> {
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.set_waitlist_enabled(request.enabled)
            .await
            .map_err(|error| AdminError::internal(format!("settings error: {error}")))
    }
    .await;

    let audit_result = AdminAuditResult::from(result.is_ok());
    emit_admin_audit(
        &state,
        AdminAuditEvent::new(
            Some(admin.id),
            "settings.waitlist.update",
            "settings",
            "waitlist",
            audit_result,
            &ctx,
        ),
    )
    .await;

    let settings = result?;
    Ok((StatusCode::OK, Json(settings_response(settings))))
}
