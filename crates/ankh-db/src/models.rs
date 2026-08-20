//! Shared identity database models.

use std::time::Duration;

use ankh_types::{
    DeviceAuthGrantId, DevicePlatform, DeviceSessionId, NamespaceId, OrgId, OrgInviteId, OrgRole,
    SessionId, SysadminId, UserId,
};
use chrono::{DateTime, Utc};

/// Session metadata stored in the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Owning user email.
    pub email: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last touch timestamp.
    pub touched_at: DateTime<Utc>,
    /// Absolute expiry timestamp.
    pub expires_at: DateTime<Utc>,
}

/// Pending device authorization grant stored in the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAuthGrant {
    /// Unique identifier for the grant.
    pub id: DeviceAuthGrantId,
    /// User who authorized the grant.
    pub user_id: UserId,
    /// PKCE code challenge.
    pub code_challenge: String,
    /// State parameter echoed to the redirect.
    pub state: String,
    /// Loopback redirect port.
    pub redirect_port: i32,
    /// Device label supplied by the client.
    pub device_name: String,
    /// Platform that initiated the request.
    pub platform: DevicePlatform,
    /// Number of failed exchange attempts.
    pub attempts: i32,
    /// When the grant was created.
    pub created_at: DateTime<Utc>,
    /// When the grant expires.
    pub expires_at: DateTime<Utc>,
    /// When the grant was consumed, if ever.
    pub consumed_at: Option<DateTime<Utc>>,
}

/// Result of creating a device authorization grant, including the raw code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedDeviceAuthGrant {
    /// Unique identifier for the grant.
    pub id: DeviceAuthGrantId,
    /// Raw authorization code shown only once.
    pub code: String,
}

/// Input data for creating a device authorization grant.
#[derive(Debug, Clone)]
pub struct DeviceAuthGrantRequest<'a> {
    /// User who authorized the grant.
    pub user_id: UserId,
    /// PKCE code challenge.
    pub code_challenge: &'a str,
    /// State parameter echoed to the redirect.
    pub state: &'a str,
    /// Loopback redirect port.
    pub redirect_port: i32,
    /// Device label supplied by the client.
    pub device_name: &'a str,
    /// Platform that initiated the request.
    pub platform: DevicePlatform,
    /// Time-to-live for the grant.
    pub ttl: Duration,
}

/// Device session metadata stored in the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSession {
    /// Unique identifier for the session.
    pub id: DeviceSessionId,
    /// User that owns this session.
    pub user_id: UserId,
    /// Device label supplied by the client.
    pub device_name: String,
    /// Platform for the session.
    pub platform: DevicePlatform,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last-used timestamp.
    pub last_used_at: DateTime<Utc>,
    /// Absolute expiry timestamp.
    pub expires_at: DateTime<Utc>,
    /// When the session was revoked, if revoked.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Result of creating a device session, including the raw token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedDeviceSession {
    /// Device session metadata.
    pub session: DeviceSession,
    /// Raw device session token shown only once.
    pub token: String,
}

/// Token purpose used for one-time identity actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Email verification token created after signup.
    EmailVerification,
    /// Password reset token created by the forgot-password flow.
    PasswordReset,
}

impl TokenKind {
    /// Return the stable database storage value for this token kind.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmailVerification => "email_verification",
            Self::PasswordReset => "password_reset",
        }
    }
}

/// Sysadmin account metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SysadminInfo {
    /// Unique identifier for the sysadmin account.
    pub id: SysadminId,
    /// Sysadmin email address.
    pub email: String,
    /// When the sysadmin account was created.
    pub created_at: DateTime<Utc>,
    /// When the sysadmin last logged in, if ever.
    pub last_login_at: Option<DateTime<Utc>>,
    /// When the sysadmin account was disabled, if disabled.
    pub disabled_at: Option<DateTime<Utc>>,
}

/// Global identity settings stored in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSettings {
    /// Whether waitlist mode is enabled.
    pub waitlist_enabled: bool,
}

/// Namespace status change returned after updating edge-visible namespace state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceStatusUpdate {
    /// Namespace name used as the durable identity.
    pub name: String,
    /// Whether the namespace is suspended after the update.
    pub suspended: bool,
    /// Namespace generation after the update.
    pub r#gen: i64,
}

/// Detailed user information for admin viewing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDetail {
    /// Unique identifier for the user.
    pub id: UserId,
    /// User's namespace ID.
    pub namespace_id: NamespaceId,
    /// User's unique username.
    pub username: String,
    /// User email address.
    pub email: String,
    /// When the user account was created.
    pub created_at: DateTime<Utc>,
    /// When the email was verified, if ever.
    pub verified_at: Option<DateTime<Utc>>,
    /// Most recent session activity, if any sessions exist.
    pub last_session_at: Option<DateTime<Utc>>,
}

/// Summary user information for listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSummary {
    /// Unique identifier for the user.
    pub id: UserId,
    /// User's namespace ID.
    pub namespace_id: NamespaceId,
    /// User's unique username.
    pub username: String,
    /// User email address.
    pub email: String,
    /// When the user account was created.
    pub created_at: DateTime<Utc>,
    /// When the email was verified, if ever.
    pub verified_at: Option<DateTime<Utc>>,
}

/// Organization summary information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgSummary {
    /// Unique identifier for the organization.
    pub id: OrgId,
    /// Namespace ID for the organization.
    pub namespace_id: NamespaceId,
    /// Organization's unique name.
    pub name: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// When the organization was created.
    pub created_at: DateTime<Utc>,
}

/// Detailed organization information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgDetail {
    /// Unique identifier for the organization.
    pub id: OrgId,
    /// Namespace ID for the organization.
    pub namespace_id: NamespaceId,
    /// Organization's unique name.
    pub name: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// User ID who created the organization, if the creator still exists.
    pub created_by: Option<UserId>,
    /// Namespace status for edge and admin views.
    pub namespace_status: String,
    /// Edge-visible namespace generation.
    pub namespace_gen: i64,
    /// When the organization was created.
    pub created_at: DateTime<Utc>,
    /// When the organization was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Organization membership information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgMember {
    /// User ID of the member.
    pub user_id: UserId,
    /// Username of the member.
    pub username: String,
    /// Email of the member.
    pub email: String,
    /// Role within the organization.
    pub role: OrgRole,
    /// User ID who added this member, if known.
    pub added_by: Option<UserId>,
    /// When the membership was created.
    pub created_at: DateTime<Utc>,
    /// When the membership was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Organization invite information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgInvite {
    /// Unique identifier for the invite.
    pub id: OrgInviteId,
    /// Organization ID the invite is for.
    pub org_id: OrgId,
    /// Email address invited.
    pub email: String,
    /// User ID who created the invite.
    pub invited_by: UserId,
    /// When the invite was created.
    pub created_at: DateTime<Utc>,
    /// When the invite expires.
    pub expires_at: DateTime<Utc>,
    /// When the invite was accepted, if ever.
    pub accepted_at: Option<DateTime<Utc>>,
    /// When the invite was revoked, if ever.
    pub revoked_at: Option<DateTime<Utc>>,
    /// User ID who accepted the invite, if accepted.
    pub accepted_by: Option<UserId>,
}

/// Session status for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// Session is active.
    Active,
    /// Session has been revoked.
    Revoked,
    /// Session has expired.
    Expired,
}

/// Summary session information for listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// Unique identifier for the session.
    pub id: SessionId,
    /// User ID that owns this session.
    pub user_id: UserId,
    /// User email.
    pub user_email: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last touched.
    pub touched_at: DateTime<Utc>,
    /// When the session expires.
    pub expires_at: DateTime<Utc>,
    /// When the session was revoked, if ever.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Device session status for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceSessionStatus {
    /// Session is active.
    Active,
    /// Session has been revoked.
    Revoked,
    /// Session has expired.
    Expired,
}

/// Summary device session information for listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSessionSummary {
    /// Unique identifier for the session.
    pub id: DeviceSessionId,
    /// User ID that owns this session.
    pub user_id: UserId,
    /// User email.
    pub user_email: String,
    /// Device label supplied by the client.
    pub device_name: String,
    /// Platform for the session.
    pub platform: DevicePlatform,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last used.
    pub last_used_at: DateTime<Utc>,
    /// When the session expires.
    pub expires_at: DateTime<Utc>,
    /// When the session was revoked, if ever.
    pub revoked_at: Option<DateTime<Utc>>,
}
