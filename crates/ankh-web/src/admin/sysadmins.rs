//! Shared sysadmin account handlers.

use ankh_types::admin::{ListSysadminsResponse, WhoamiResponse};
use axum::{Extension, Json, extract::Query, http::StatusCode, response::IntoResponse};
use serde::Deserialize;

use super::{
    conversions::sysadmin_summary,
    error::{AdminError, AdminResult},
    middleware::SysadminAuth,
    pagination::{clamp_limit, default_limit},
};
use crate::AnkhWebState;

/// Query parameters for listing sysadmins.
#[derive(Debug, Deserialize)]
pub struct ListSysadminsQuery {
    /// Maximum number of sysadmins to return.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Pagination cursor for fetching subsequent pages.
    pub cursor: Option<String>,
}

/// List sysadmin accounts with pagination.
pub async fn list_sysadmins(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Query(query): Query<ListSysadminsQuery>,
) -> AdminResult<impl IntoResponse> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let (sysadmins, next_cursor) = db
        .list_sysadmins(clamp_limit(query.limit), query.cursor.as_deref())
        .await
        .map_err(|error| AdminError::internal(format!("list sysadmins error: {error}")))?;

    Ok((
        StatusCode::OK,
        Json(ListSysadminsResponse {
            sysadmins: sysadmins.into_iter().map(sysadmin_summary).collect(),
            next_cursor,
        }),
    ))
}

/// Return the authenticated sysadmin identity.
pub async fn whoami(SysadminAuth(sysadmin): SysadminAuth) -> AdminResult<impl IntoResponse> {
    Ok((
        StatusCode::OK,
        Json(WhoamiResponse {
            sysadmin: sysadmin_summary(sysadmin),
        }),
    ))
}
