//! Public auth, org, and device-session payloads.

use std::{fmt, result};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use ts_rs::TS;

/// Platform identifier for generic device sessions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, TS)]
#[ts(type = "string")]
pub enum DevicePlatform {
    /// macOS native device.
    Macos,
    /// Windows native device.
    Windows,
    /// Linux native device.
    Linux,
    /// Browser or web runtime.
    Web,
    /// A platform not known to this version of Ankh.
    Other(String),
}

impl DevicePlatform {
    /// Return the stable database value for this platform.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Web => "web",
            Self::Other(value) => value.as_str(),
        }
    }

    /// Parse a platform from a database string, preserving unknown values.
    #[must_use]
    pub fn parse_db(value: &str) -> Self {
        match value {
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "web" => Self::Web,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl fmt::Display for DevicePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for DevicePlatform {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DevicePlatform {
    fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            return Err(de::Error::custom("device platform cannot be empty"));
        }
        Ok(Self::parse_db(value.as_str()))
    }
}

/// Lightweight user info returned from auth endpoints.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct UserInfo {
    /// User's unique username.
    pub username: String,
    /// User email address.
    pub email: String,
    /// Whether the email has been verified.
    pub email_verified: bool,
    /// Whether the account is currently waitlisted.
    pub waitlisted: bool,
}

/// Authentication result for JSON auth endpoints.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct AuthSuccess {
    /// Authenticated user info.
    pub user: UserInfo,
}

/// Response body for the current auth identity endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct AuthMeResponse {
    /// Current user if a web session is authenticated.
    pub user: Option<UserInfo>,
}

/// Request body for account signup.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct SignupRequest {
    /// Requested username.
    pub username: String,
    /// User email address.
    pub email: String,
    /// User password.
    pub password: String,
    /// Optional account invite token.
    pub invite_token: Option<String>,
    /// Optional organization invite token.
    pub org_invite_token: Option<String>,
}

/// Request body for user login.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct LoginRequest {
    /// Email address or username.
    pub email: String,
    /// User password.
    pub password: String,
}

/// Response body for waitlist status.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct WaitlistStatusResponse {
    /// Whether waitlist mode is currently enabled.
    pub waitlist_enabled: bool,
}

/// Request body for email verification.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct VerificationRequest {
    /// Raw verification token.
    pub token: String,
}

/// Request body for resending email verification.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct ResendVerificationRequest {
    /// Email address to send a verification link to.
    pub email: String,
}

/// Request body for password reset email creation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct ForgotPasswordRequest {
    /// Email address to send a reset link to.
    pub email: String,
}

/// Request body for password reset token validation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct ValidateResetTokenRequest {
    /// Raw password reset token.
    pub token: String,
}

/// Response body for password reset token validation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct ValidateResetTokenResponse {
    /// Email address associated with the reset token.
    pub email: String,
}

/// Request body for completing a password reset.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct PasswordResetRequest {
    /// Raw password reset token.
    pub token: String,
    /// New password.
    pub password: String,
}

/// Organization summary for UI display.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct OrgInfo {
    /// Organization ID, or namespace ID for personal organizations.
    pub id: String,
    /// Organization name.
    pub name: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// User's role in this organization.
    pub role: String,
    /// Whether this is a personal organization.
    pub is_personal: bool,
}

/// Request to create a new organization.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct CreateOrgInput {
    /// Organization name.
    pub name: String,
    /// Optional display name.
    pub display_name: Option<String>,
}

/// Current user's membership info in an organization.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct OrgMemberInfo {
    /// User ID.
    pub user_id: String,
    /// Username.
    pub username: String,
    /// Role in the organization.
    pub role: String,
}

/// Organization member info for UI display.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct MemberInfo {
    /// User ID.
    pub user_id: String,
    /// Username.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Role in the organization.
    pub role: String,
}

/// Pending organization invite for UI display.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct InviteInfo {
    /// Invite ID.
    pub id: String,
    /// Email address invited.
    pub email: String,
    /// When the invite was created.
    pub created_at: String,
    /// When the invite expires.
    pub expires_at: String,
}

/// Organization invite details for acceptance flows.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct OrgInviteDetails {
    /// Organization name.
    pub org_name: String,
    /// Organization display name.
    pub org_display_name: Option<String>,
    /// Email the invite was sent to.
    pub invite_email: String,
}

/// Response body for accepting an organization invite.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct AcceptOrgInviteResponse {
    /// Organization joined by accepting the invite.
    pub org: OrgInfo,
}

/// Device session info for UI display.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct DeviceSessionInfo {
    /// Session ID.
    pub id: String,
    /// Device name.
    pub device_name: String,
    /// Platform identifier.
    pub platform: DevicePlatform,
    /// Status: active, revoked, or expired.
    pub status: String,
    /// When the session was created.
    pub created_at: String,
    /// When the session was last used.
    pub last_used_at: String,
    /// When the session expires.
    pub expires_at: String,
}

/// Response body for minting a device session from an authenticated web session.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct CreateDeviceSessionResponse {
    /// Raw bearer token returned once.
    pub token: String,
    /// Device label associated with the session.
    pub device_name: String,
    /// When the token expires.
    pub expires_at: String,
}

/// Query parameters for the browser device authorization flow.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct DeviceAuthorizationRequest {
    /// PKCE S256 code challenge.
    pub code_challenge: String,
    /// Loopback redirect port.
    pub redirect_port: u16,
    /// Caller-provided state echoed back to the loopback callback.
    pub state: String,
    /// Human-readable device name.
    pub device_name: String,
    /// Platform identifier supplied by the device client.
    pub platform: DevicePlatform,
}

/// Device authorization grant details returned internally by services.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct DeviceAuthorizationResponse {
    /// Raw one-time authorization code.
    pub code: String,
    /// Loopback callback URL.
    pub callback_url: String,
    /// When the grant expires.
    pub expires_at: DateTime<Utc>,
}

/// Request body for exchanging a device authorization code.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct DeviceTokenRequest {
    /// Raw one-time authorization code.
    pub code: String,
    /// PKCE verifier corresponding to the authorization challenge.
    pub code_verifier: String,
}

/// Response body for a device authorization token exchange.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
pub struct DeviceTokenResponse {
    /// Raw bearer token returned once.
    pub token: String,
    /// Human-readable device name.
    pub device_name: String,
    /// Platform identifier stored for the session.
    pub platform: DevicePlatform,
    /// When the device session expires.
    pub expires_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    //! Tests for public DTO serialization behavior.

    use serde_json::json;

    use super::{DevicePlatform, DeviceSessionInfo};

    /// Proves known device platforms serialize as stable strings.
    #[test]
    fn device_platform_serializes_as_string() {
        let serialized = serde_json::to_value(&DevicePlatform::Web).expect("serialize platform");

        assert_eq!(serialized, json!("web"));
    }

    /// Proves unknown device platforms are preserved instead of rejected.
    #[test]
    fn device_platform_preserves_unknown_database_values() {
        let parsed: DevicePlatform =
            serde_json::from_value(json!("plan9")).expect("deserialize platform");

        assert_eq!(parsed, DevicePlatform::Other("plan9".to_owned()));
        assert_eq!(parsed.as_str(), "plan9");
    }

    /// Proves DTOs use generic device-session naming.
    #[test]
    fn device_session_info_uses_generic_platform() {
        let session = DeviceSessionInfo {
            id: "session-id".to_owned(),
            device_name: "Browser Player".to_owned(),
            platform: DevicePlatform::Web,
            status: "active".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            last_used_at: "2026-01-01T00:00:00Z".to_owned(),
            expires_at: "2026-01-02T00:00:00Z".to_owned(),
        };

        assert_eq!(session.platform, DevicePlatform::Web);
    }
}
