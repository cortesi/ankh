//! Shared admin API request and response payloads.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Request body for sysadmin login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AdminLoginRequest {
    /// Sysadmin email address.
    pub email: String,
    /// Sysadmin password.
    pub password: String,
}

/// Sysadmin identity returned by authentication flows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SysadminIdentity {
    /// Sysadmin ID.
    pub id: String,
    /// Sysadmin email address.
    pub email: String,
}

/// Response body for successful admin login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LoginResponse {
    /// Bearer token to use for authenticated requests.
    pub token: String,
    /// Token expiration timestamp.
    pub expires_at: DateTime<Utc>,
    /// Authenticated sysadmin identity.
    pub sysadmin: SysadminIdentity,
}

/// Sysadmin summary in list responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SysadminSummary {
    /// Sysadmin ID.
    pub id: String,
    /// Sysadmin email address.
    pub email: String,
    /// When the sysadmin was created.
    pub created_at: DateTime<Utc>,
    /// When the sysadmin last logged in, if ever.
    pub last_login_at: Option<DateTime<Utc>>,
}

/// Response body for listing sysadmins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListSysadminsResponse {
    /// List of sysadmins.
    pub sysadmins: Vec<SysadminSummary>,
    /// Cursor for fetching the next page, if more results exist.
    pub next_cursor: Option<String>,
}

/// Response body for the authenticated sysadmin identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WhoamiResponse {
    /// Authenticated sysadmin info.
    pub sysadmin: SysadminSummary,
}

/// User summary in list responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserSummary {
    /// User ID.
    pub id: String,
    /// User's unique username.
    pub username: String,
    /// User email address.
    pub email: String,
    /// When the user was created.
    pub created_at: DateTime<Utc>,
    /// When the email was verified, if ever.
    pub verified_at: Option<DateTime<Utc>>,
}

/// Response body for listing users.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListUsersResponse {
    /// List of users.
    pub users: Vec<UserSummary>,
    /// Cursor for fetching the next page, if more results exist.
    pub next_cursor: Option<String>,
}

/// User detail in get responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserDetail {
    /// User ID.
    pub id: String,
    /// User's unique username.
    pub username: String,
    /// User email address.
    pub email: String,
    /// When the user was created.
    pub created_at: DateTime<Utc>,
    /// When the email was verified, if ever.
    pub verified_at: Option<DateTime<Utc>>,
    /// Most recent session activity, if any sessions exist.
    pub last_session_at: Option<DateTime<Utc>>,
}

/// Request body for releasing a waitlisted user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReleaseUserRequest {
    /// User ID to release.
    pub id: Option<String>,
    /// User email to release.
    pub email: Option<String>,
}

/// Response body for releasing a waitlisted user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReleaseUserResponse {
    /// User email address.
    pub email: String,
}

/// Request body for inviting a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InviteUserRequest {
    /// User email to invite.
    pub email: String,
}

/// Action taken by the invite endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InviteAction {
    /// Invite email sent.
    Invited,
    /// Existing waitlisted user released.
    Released,
    /// User already active.
    AlreadyActive,
}

/// Response body for inviting a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InviteUserResponse {
    /// User email address.
    pub email: String,
    /// Action taken for the invite request.
    pub action: InviteAction,
}

/// Session summary in list responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SessionSummary {
    /// Session ID.
    pub id: String,
    /// User ID that owns this session.
    pub user_id: String,
    /// User email.
    pub user_email: String,
    /// Computed status based on revoked_at and expires_at.
    pub status: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last used.
    pub last_seen_at: DateTime<Utc>,
    /// When the session expires.
    pub expires_at: DateTime<Utc>,
    /// When the session was revoked, if ever.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Response body for listing sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListSessionsResponse {
    /// List of sessions.
    pub sessions: Vec<SessionSummary>,
    /// Cursor for fetching the next page, if more results exist.
    pub next_cursor: Option<String>,
}

/// Device session summary in admin list responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeviceSessionSummary {
    /// Session ID.
    pub id: String,
    /// User ID that owns this session.
    pub user_id: String,
    /// User email.
    pub user_email: String,
    /// Device name for the session.
    pub device_name: String,
    /// Platform identifier.
    pub platform: String,
    /// Computed status based on revoked_at and expires_at.
    pub status: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last used.
    pub last_used_at: DateTime<Utc>,
    /// When the session expires.
    pub expires_at: DateTime<Utc>,
    /// When the session was revoked, if ever.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Response body for listing device sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListDeviceSessionsResponse {
    /// List of device sessions.
    pub sessions: Vec<DeviceSessionSummary>,
    /// Cursor for fetching the next page, if more results exist.
    pub next_cursor: Option<String>,
}

/// Organization summary in list responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OrgSummary {
    /// Organization ID.
    pub id: String,
    /// Organization name.
    pub name: String,
    /// Display name.
    pub display_name: Option<String>,
    /// When the org was created.
    pub created_at: DateTime<Utc>,
}

/// Response body for listing orgs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListOrgsResponse {
    /// List of organizations.
    pub orgs: Vec<OrgSummary>,
    /// Cursor for fetching the next page.
    pub next_cursor: Option<String>,
}

/// Organization detail in get responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OrgDetail {
    /// Organization ID.
    pub id: String,
    /// Organization name.
    pub name: String,
    /// Display name.
    pub display_name: Option<String>,
    /// User who created the org, if the creator still exists.
    pub created_by: Option<String>,
    /// Namespace ID owned by the org.
    pub namespace_id: String,
    /// Namespace status.
    pub namespace_status: String,
    /// Namespace generation.
    pub namespace_gen: i64,
    /// When the org was created.
    pub created_at: DateTime<Utc>,
    /// When the org was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Request body for creating an org.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreateOrgRequest {
    /// Organization name.
    pub name: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// User ID who will be the owner.
    pub owner_id: String,
}

/// Request body for updating an org.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UpdateOrgRequest {
    /// New display name.
    pub display_name: Option<String>,
}

/// Organization member in response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OrgMember {
    /// User ID.
    pub user_id: String,
    /// Username.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Role in the organization.
    pub role: String,
    /// When the member was added.
    pub created_at: DateTime<Utc>,
}

/// Response body for listing org members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListMembersResponse {
    /// List of members.
    pub members: Vec<OrgMember>,
}

/// Request body for adding a member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AddMemberRequest {
    /// User ID to add.
    pub user_id: String,
    /// Role for the new member.
    #[serde(default = "default_member_role")]
    pub role: String,
}

/// Request body for setting a member's role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SetRoleRequest {
    /// New role.
    pub role: String,
}

/// Request body for transferring ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TransferOwnershipRequest {
    /// User ID of new owner.
    pub new_owner_id: String,
}

/// Organization invite in response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OrgInvite {
    /// Invite ID.
    pub id: String,
    /// Invited email.
    pub email: String,
    /// When the invite was created.
    pub created_at: DateTime<Utc>,
    /// When the invite expires.
    pub expires_at: DateTime<Utc>,
    /// When the invite was accepted, if ever.
    pub accepted_at: Option<DateTime<Utc>>,
    /// When the invite was revoked, if ever.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Response body for listing org invites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListOrgInvitesResponse {
    /// List of invites.
    pub invites: Vec<OrgInvite>,
}

/// Request body for creating an org invite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreateOrgInviteRequest {
    /// Email to invite.
    pub email: String,
}

/// Response body for creating an org invite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreateOrgInviteResponse {
    /// Invite ID.
    pub id: String,
    /// Invited email.
    pub email: String,
    /// Raw token returned only on creation.
    pub token: String,
}

/// Response body for settings queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SettingsResponse {
    /// Whether waitlist mode is enabled.
    pub waitlist_enabled: bool,
}

/// Request body for updating waitlist settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WaitlistSettingsRequest {
    /// Desired waitlist enablement.
    pub enabled: bool,
}

/// Response body for namespace status changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NamespaceStatusResponse {
    /// Namespace ID.
    pub id: String,
    /// Namespace status.
    pub status: String,
    /// Namespace generation after the mutation.
    #[serde(rename = "gen")]
    #[ts(rename = "gen")]
    pub r#gen: i64,
}

/// Default org member role for add-member requests.
fn default_member_role() -> String {
    "member".to_owned()
}

#[cfg(test)]
mod tests {
    //! Tests for admin DTO behavior.

    use serde_json::json;

    use super::{AddMemberRequest, InviteAction};

    /// Proves invite actions serialize with the existing snake-case contract.
    #[test]
    fn invite_action_serializes_snake_case() {
        let serialized =
            serde_json::to_value(&InviteAction::AlreadyActive).expect("serialize invite action");

        assert_eq!(serialized, json!("already_active"));
    }

    /// Proves member add requests default to member role.
    #[test]
    fn add_member_defaults_to_member_role() {
        let request: AddMemberRequest =
            serde_json::from_value(json!({ "user_id": "user-1" })).expect("deserialize request");

        assert_eq!(request.role, "member");
    }
}
