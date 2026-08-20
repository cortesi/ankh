//! Shared admin API extractors.

use ankh_db::{Error as DbError, SysadminInfo};
use axum::{
    extract::{Extension, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};

use super::error::AdminError;
use crate::AnkhWebState;

/// Extractor that validates an admin bearer token and provides the sysadmin info.
pub struct SysadminAuth(
    /// Authenticated sysadmin info.
    pub SysadminInfo,
);

impl<S> FromRequestParts<S> for SysadminAuth
where
    S: Send + Sync,
{
    type Rejection = AdminError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Extension(ankh_state) = Extension::<AnkhWebState>::from_request_parts(parts, state)
            .await
            .map_err(|_| AdminError::internal("Ankh web state not available"))?;

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AdminError::unauthorized("missing authorization header"))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .or_else(|| auth_header.strip_prefix("bearer "))
            .ok_or_else(|| AdminError::unauthorized("invalid authorization header format"))?;

        if token.is_empty() {
            return Err(AdminError::unauthorized("empty bearer token"));
        }

        let db = ankh_state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        let admin_info = db
            .validate_sysadmin_token(token)
            .await
            .map_err(|error| match error {
                DbError::SysadminTokenNotFound(_) | DbError::SysadminTokenExpired(_) => {
                    AdminError::unauthorized("invalid or expired token")
                }
                DbError::SysadminDisabled(email) => {
                    AdminError::forbidden(format!("sysadmin account disabled: {email}"))
                }
                _ => AdminError::internal(format!("authentication error: {error}")),
            })?;

        Ok(Self(admin_info))
    }
}
