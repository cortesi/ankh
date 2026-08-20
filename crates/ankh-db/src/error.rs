//! Error types for Ankh database operations.

use std::result;

use argon2::password_hash;

/// Result type for Ankh database operations.
pub type Result<T> = result::Result<T, Error>;

/// Errors returned by Ankh database operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Postgres connection parameters could not be parsed.
    #[error("invalid postgres config: {0}")]
    InvalidPostgresConfig(String),
    /// The Postgres pool could not be built.
    #[error("postgres pool build error: {0}")]
    PoolBuild(#[from] deadpool_postgres::BuildError),
    /// A pooled Postgres client could not be checked out.
    #[error("postgres pool error: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),
    /// Password hashing failed.
    #[error("password hashing failed: {0}")]
    PasswordHash(#[from] password_hash::Error),
    /// Stored waitlist status is not recognized.
    #[error("invalid waitlist status: {0}")]
    InvalidWaitlistStatus(String),
    /// No schema version has been recorded yet.
    #[error("schema version not set")]
    SchemaVersionMissing,
    /// Email already exists when attempting to create a user.
    #[error("user {0} already exists")]
    UserExists(String),
    /// Requested user could not be found.
    #[error("user {0} not found")]
    UserMissing(String),
    /// Requested session could not be found.
    #[error("session {0} not found")]
    SessionMissing(String),
    /// Session already exists when attempting to create one.
    #[error("session {0} already exists")]
    SessionExists(String),
    /// Device auth grant could not be found.
    #[error("device auth grant {0} not found")]
    DeviceAuthGrantMissing(String),
    /// Device auth grant has expired.
    #[error("device auth grant {0} expired")]
    DeviceAuthGrantExpired(String),
    /// Device auth grant was already consumed.
    #[error("device auth grant {0} already consumed")]
    DeviceAuthGrantConsumed(String),
    /// Device auth grant exceeded allowed attempts.
    #[error("device auth grant {0} attempts exceeded")]
    DeviceAuthGrantAttemptsExceeded(String),
    /// Device auth grant verifier did not match.
    #[error("device auth grant {0} verifier mismatch")]
    DeviceAuthGrantInvalidVerifier(String),
    /// Device session could not be found.
    #[error("device session {0} not found")]
    DeviceSessionMissing(String),
    /// Device session has expired.
    #[error("device session {0} expired")]
    DeviceSessionExpired(String),
    /// Device session has been revoked.
    #[error("device session {0} revoked")]
    DeviceSessionRevoked(String),
    /// Device session limit reached for user.
    #[error("device session limit reached for user {0}")]
    DeviceSessionLimitReached(String),
    /// Authentication failed due to invalid credentials.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// Token could not be found.
    #[error("token not found: {0}")]
    TokenNotFound(String),
    /// Token is present but has expired.
    #[error("token expired: {0}")]
    TokenExpired(String),
    /// Invite token could not be found.
    #[error("invite not found: {0}")]
    InviteNotFound(String),
    /// Invite token is present but has expired.
    #[error("invite expired: {0}")]
    InviteExpired(String),
    /// Operation requires a verified email address.
    #[error("email not verified: {0}")]
    EmailNotVerified(String),
    /// Sysadmin account with this email already exists.
    #[error("sysadmin {0} already exists")]
    SysadminExists(String),
    /// Requested sysadmin account could not be found.
    #[error("sysadmin {0} not found")]
    SysadminMissing(String),
    /// Sysadmin token could not be found or is invalid.
    #[error("sysadmin token not found: {0}")]
    SysadminTokenNotFound(String),
    /// Sysadmin token is present but has expired.
    #[error("sysadmin token expired: {0}")]
    SysadminTokenExpired(String),
    /// Sysadmin account is disabled.
    #[error("sysadmin account disabled: {0}")]
    SysadminDisabled(String),
    /// Namespace name already exists.
    #[error("namespace {0} already exists")]
    NamespaceExists(String),
    /// Requested namespace could not be found.
    #[error("namespace {0} not found")]
    NamespaceMissing(String),
    /// Invalid namespace name.
    #[error("invalid namespace name: {0}")]
    InvalidNamespaceName(&'static str),
    /// Organization could not be found.
    #[error("organization {0} not found")]
    OrgMissing(String),
    /// Organization already exists.
    #[error("organization {0} already exists")]
    OrgExists(String),
    /// Organization invite could not be found.
    #[error("org invite not found: {0}")]
    OrgInviteNotFound(String),
    /// Organization invite has expired.
    #[error("org invite expired: {0}")]
    OrgInviteExpired(String),
    /// User is not a member of the organization.
    #[error("user {0} is not a member of organization {1}")]
    NotOrgMember(String, String),
    /// User lacks permission for this operation.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// Organization still has members and cannot be deleted.
    #[error("organization {0} is not empty")]
    OrgNotEmpty(String),
    /// A pending invite already exists for this email.
    #[error("pending invite already exists for {0}")]
    OrgInviteAlreadyPending(String),
    /// Email does not match the invite email.
    #[error("email mismatch: expected {expected}, got {actual}")]
    EmailMismatch {
        /// Expected email address.
        expected: String,
        /// Actual email address.
        actual: String,
    },
    /// User is already a member of the organization.
    #[error("user {0} is already a member of organization {1}")]
    AlreadyOrgMember(String, String),
    /// Invite has already been accepted.
    #[error("invite already accepted")]
    OrgInviteAlreadyAccepted,
    /// Invite has been revoked.
    #[error("invite has been revoked")]
    OrgInviteRevoked,
    /// Postgres returned an error.
    #[error("postgres error: {0}")]
    Postgres(#[from] tokio_postgres::Error),
}
