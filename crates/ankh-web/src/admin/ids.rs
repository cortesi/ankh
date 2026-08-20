//! ID parsers for shared admin routes.

use ankh_db::{DeviceSessionId, NamespaceId, OrgId, OrgInviteId, SessionId, UserId};

use super::error::AdminError;

/// Parse a user ID from a route segment.
pub fn parse_user_id(id: &str) -> Result<UserId, AdminError> {
    id.parse()
        .map_err(|_| AdminError::bad_request("invalid user id format"))
}

/// Parse a web session ID from a route segment.
pub fn parse_session_id(id: &str) -> Result<SessionId, AdminError> {
    id.parse()
        .map_err(|_| AdminError::bad_request("invalid session id format"))
}

/// Parse a device session ID from a route segment.
pub fn parse_device_session_id(id: &str) -> Result<DeviceSessionId, AdminError> {
    id.parse()
        .map_err(|_| AdminError::bad_request("invalid device session id format"))
}

/// Parse an organization ID from a route segment.
pub fn parse_org_id(id: &str) -> Result<OrgId, AdminError> {
    id.parse()
        .map_err(|_| AdminError::bad_request("invalid org id format"))
}

/// Parse an organization invite ID from a route segment.
pub fn parse_org_invite_id(id: &str) -> Result<OrgInviteId, AdminError> {
    id.parse()
        .map_err(|_| AdminError::bad_request("invalid invite id format"))
}

/// Parse a namespace ID from a route segment.
pub fn parse_namespace_id(id: &str) -> Result<NamespaceId, AdminError> {
    id.parse()
        .map_err(|_| AdminError::bad_request("invalid namespace id format"))
}
