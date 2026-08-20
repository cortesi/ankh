#![warn(missing_docs)]

//! Shared API payload and identifier types for Ankh services.

pub mod admin;
mod ids;
mod public;
pub mod ts;

pub use ids::{
    DeviceAuthGrantId, DeviceSessionId, InviteId, NamespaceId, NamespaceKind, OrgId, OrgInviteId,
    OrgRole, SessionId, SysadminId, SysadminTokenId, TokenId, UserId,
};
pub use public::{
    AcceptOrgInviteResponse, AuthMeResponse, AuthSuccess, CreateDeviceSessionResponse,
    CreateOrgInput, DeviceAuthorizationRequest, DeviceAuthorizationResponse, DevicePlatform,
    DeviceSessionInfo, DeviceTokenRequest, DeviceTokenResponse, ForgotPasswordRequest, InviteInfo,
    LoginRequest, MemberInfo, OrgInfo, OrgInviteDetails, OrgMemberInfo, PasswordResetRequest,
    ResendVerificationRequest, SignupRequest, UserInfo, ValidateResetTokenRequest,
    ValidateResetTokenResponse, VerificationRequest, WaitlistStatusResponse,
};
