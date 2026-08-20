#![warn(missing_docs)]

//! Shared identity, session, device, mail, and admin constants.

use std::time::Duration;

/// Default time-to-live for newly issued browser sessions.
pub const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 30);

/// Idle interval after which a session touch updates its last-used timestamp.
pub const SESSION_TOUCH_STALE_AFTER: Duration = Duration::from_secs(15 * 60);

/// Time-to-live for email verification tokens.
pub const EMAIL_VERIFICATION_TTL: Duration = Duration::from_secs(60 * 60 * 24);

/// Time-to-live for password reset tokens.
pub const PASSWORD_RESET_TTL: Duration = Duration::from_secs(60 * 60);

/// Cooldown between verification resend attempts.
pub const VERIFICATION_RESEND_COOLDOWN: Duration = Duration::from_secs(60);

/// Time-to-live for organization invite links.
pub const ORG_INVITE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Time-to-live for account invite links created by admins.
pub const USER_INVITE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Default time-to-live for sysadmin tokens.
pub const DEFAULT_SYSADMIN_TOKEN_TTL: Duration = Duration::from_secs(60 * 60 * 24);

/// Time-to-live for device authorization grants.
pub const DEVICE_AUTH_GRANT_TTL: Duration = Duration::from_secs(60 * 10);

/// Maximum verification attempts allowed per device authorization grant.
pub const DEVICE_AUTH_GRANT_MAX_ATTEMPTS: u32 = 5;

/// Time-to-live for device sessions.
pub const DEVICE_SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 90);

/// Maximum number of concurrent device sessions per user.
pub const DEVICE_SESSION_LIMIT: u32 = 10;

/// Maximum length of a device session label.
pub const DEVICE_NAME_MAX_LEN: usize = 64;

/// Minimum accepted password length.
pub const MIN_PASSWORD_LEN: usize = 8;

/// Per-email login attempts allowed each minute.
pub const USER_LOGIN_RATE_PER_MINUTE: u32 = 20;

/// Total login attempts allowed each minute.
pub const USER_LOGIN_GLOBAL_PER_MINUTE: u32 = 300;

/// Per-email signup attempts allowed each hour.
pub const SIGNUP_RATE_PER_HOUR: u32 = 10;

/// Total signup attempts allowed each hour.
pub const SIGNUP_GLOBAL_PER_HOUR: u32 = 100;

/// Per-email password reset attempts allowed each hour.
pub const PASSWORD_RESET_RATE_PER_HOUR: u32 = 10;

/// Total password reset attempts allowed each hour.
pub const PASSWORD_RESET_GLOBAL_PER_HOUR: u32 = 100;

/// Per-IP device authorization exchange attempts allowed each minute.
pub const DEVICE_AUTH_EXCHANGE_RATE_PER_MINUTE: u32 = 20;

/// Total sysadmin login attempts allowed each minute.
pub const ADMIN_LOGIN_GLOBAL_RATE_PER_MINUTE: u32 = 10;

/// Default page size for admin list endpoints.
pub const ADMIN_LIST_DEFAULT_LIMIT: i64 = 50;

/// Maximum page size accepted by admin list endpoints.
pub const ADMIN_LIST_MAX_LIMIT: i64 = 100;

/// Session defaults grouped for service configuration.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct WebSessionConfig {
    /// Time-to-live for newly issued browser sessions.
    pub ttl: Duration,
    /// Idle interval after which a session touch updates `last_seen_at`.
    pub touch_stale_after: Duration,
}

impl Default for WebSessionConfig {
    fn default() -> Self {
        Self {
            ttl: DEFAULT_SESSION_TTL,
            touch_stale_after: SESSION_TOUCH_STALE_AFTER,
        }
    }
}

/// Device authorization and session defaults grouped for service configuration.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DeviceSessionConfig {
    /// Time-to-live for device authorization grants.
    pub grant_ttl: Duration,
    /// Maximum verification attempts allowed per authorization grant.
    pub grant_max_attempts: u32,
    /// Time-to-live for device bearer sessions.
    pub session_ttl: Duration,
    /// Maximum concurrent device sessions per user.
    pub session_limit: u32,
    /// Maximum length of a device label.
    pub device_name_max_len: usize,
}

impl Default for DeviceSessionConfig {
    fn default() -> Self {
        Self {
            grant_ttl: DEVICE_AUTH_GRANT_TTL,
            grant_max_attempts: DEVICE_AUTH_GRANT_MAX_ATTEMPTS,
            session_ttl: DEVICE_SESSION_TTL,
            session_limit: DEVICE_SESSION_LIMIT,
            device_name_max_len: DEVICE_NAME_MAX_LEN,
        }
    }
}

/// Identity token defaults grouped for service configuration.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IdentityTokenConfig {
    /// Time-to-live for email verification tokens.
    pub email_verification_ttl: Duration,
    /// Time-to-live for password reset tokens.
    pub password_reset_ttl: Duration,
    /// Cooldown between verification resend attempts.
    pub verification_resend_cooldown: Duration,
    /// Time-to-live for organization invite links.
    pub org_invite_ttl: Duration,
    /// Time-to-live for account invite links.
    pub user_invite_ttl: Duration,
}

impl Default for IdentityTokenConfig {
    fn default() -> Self {
        Self {
            email_verification_ttl: EMAIL_VERIFICATION_TTL,
            password_reset_ttl: PASSWORD_RESET_TTL,
            verification_resend_cooldown: VERIFICATION_RESEND_COOLDOWN,
            org_invite_ttl: ORG_INVITE_TTL,
            user_invite_ttl: USER_INVITE_TTL,
        }
    }
}

/// Password policy defaults.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PasswordPolicy {
    /// Minimum accepted password length.
    pub min_len: usize,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_len: MIN_PASSWORD_LEN,
        }
    }
}

/// Auth endpoint rate-limit defaults.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AuthRateLimits {
    /// Per-email login attempts allowed each minute.
    pub user_login_per_minute: u32,
    /// Total login attempts allowed each minute.
    pub user_login_global_per_minute: u32,
    /// Per-email signup attempts allowed each hour.
    pub signup_per_hour: u32,
    /// Total signup attempts allowed each hour.
    pub signup_global_per_hour: u32,
    /// Per-email password reset attempts allowed each hour.
    pub password_reset_per_hour: u32,
    /// Total password reset attempts allowed each hour.
    pub password_reset_global_per_hour: u32,
    /// Per-IP device authorization exchange attempts allowed each minute.
    pub device_auth_exchange_per_minute: u32,
}

impl Default for AuthRateLimits {
    fn default() -> Self {
        Self {
            user_login_per_minute: USER_LOGIN_RATE_PER_MINUTE,
            user_login_global_per_minute: USER_LOGIN_GLOBAL_PER_MINUTE,
            signup_per_hour: SIGNUP_RATE_PER_HOUR,
            signup_global_per_hour: SIGNUP_GLOBAL_PER_HOUR,
            password_reset_per_hour: PASSWORD_RESET_RATE_PER_HOUR,
            password_reset_global_per_hour: PASSWORD_RESET_GLOBAL_PER_HOUR,
            device_auth_exchange_per_minute: DEVICE_AUTH_EXCHANGE_RATE_PER_MINUTE,
        }
    }
}

/// Admin endpoint defaults.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AdminConfig {
    /// Default sysadmin token time-to-live.
    pub token_ttl: Duration,
    /// Total sysadmin login attempts allowed each minute.
    pub login_global_per_minute: u32,
    /// Default page size for admin list endpoints.
    pub list_default_limit: i64,
    /// Maximum page size accepted by admin list endpoints.
    pub list_max_limit: i64,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            token_ttl: DEFAULT_SYSADMIN_TOKEN_TTL,
            login_global_per_minute: ADMIN_LOGIN_GLOBAL_RATE_PER_MINUTE,
            list_default_limit: ADMIN_LIST_DEFAULT_LIMIT,
            list_max_limit: ADMIN_LIST_MAX_LIMIT,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for default identity constants.

    use super::{
        ADMIN_LIST_DEFAULT_LIMIT, ADMIN_LIST_MAX_LIMIT, AdminConfig, AuthRateLimits,
        DEVICE_SESSION_LIMIT, DeviceSessionConfig, IdentityTokenConfig, MIN_PASSWORD_LEN,
        PasswordPolicy, WebSessionConfig,
    };

    /// Proves default config structs reflect their constants.
    #[test]
    fn config_defaults_match_constants() {
        assert_eq!(WebSessionConfig::default().ttl.as_secs(), 60 * 60 * 24 * 30);
        assert_eq!(
            DeviceSessionConfig::default().session_limit,
            DEVICE_SESSION_LIMIT
        );
        assert_eq!(
            IdentityTokenConfig::default().password_reset_ttl.as_secs(),
            60 * 60
        );
        assert_eq!(PasswordPolicy::default().min_len, MIN_PASSWORD_LEN);
        assert_eq!(
            AuthRateLimits::default().device_auth_exchange_per_minute,
            20
        );
        assert_eq!(
            AdminConfig::default().list_default_limit,
            ADMIN_LIST_DEFAULT_LIMIT
        );
        assert_eq!(AdminConfig::default().list_max_limit, ADMIN_LIST_MAX_LIMIT);
    }
}
