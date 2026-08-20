//! Conversions from Ankh database models into admin API payloads.

use ankh_db::{
    AppSettings, DeviceSessionSummary as DbDeviceSessionSummary, OrgDetail as DbOrgDetail,
    OrgInvite as DbOrgInvite, OrgMember as DbOrgMember, OrgSummary as DbOrgSummary,
    SessionSummary as DbSessionSummary, SysadminInfo, UserDetail as DbUserDetail,
    UserSummary as DbUserSummary,
};
use ankh_types::admin as api;
use chrono::{DateTime, Utc};

/// Build an admin sysadmin summary from database info.
#[must_use]
pub fn sysadmin_summary(info: SysadminInfo) -> api::SysadminSummary {
    api::SysadminSummary {
        id: info.id.to_string(),
        email: info.email,
        created_at: info.created_at,
        last_login_at: info.last_login_at,
    }
}

/// Build an admin sysadmin identity from database info.
#[must_use]
pub fn sysadmin_identity(info: SysadminInfo) -> api::SysadminIdentity {
    api::SysadminIdentity {
        id: info.id.to_string(),
        email: info.email,
    }
}

/// Build an admin user summary from a database summary.
#[must_use]
pub fn user_summary(summary: DbUserSummary) -> api::UserSummary {
    api::UserSummary {
        id: summary.id.to_string(),
        username: summary.username,
        email: summary.email,
        created_at: summary.created_at,
        verified_at: summary.verified_at,
    }
}

/// Build an admin user detail from a database detail.
#[must_use]
pub fn user_detail(detail: DbUserDetail) -> api::UserDetail {
    api::UserDetail {
        id: detail.id.to_string(),
        username: detail.username,
        email: detail.email,
        created_at: detail.created_at,
        verified_at: detail.verified_at,
        last_session_at: detail.last_session_at,
    }
}

/// Build an admin session summary using the specified timestamp for status evaluation.
#[must_use]
pub fn session_summary_at(summary: DbSessionSummary, now: DateTime<Utc>) -> api::SessionSummary {
    let status = session_status(summary.revoked_at, summary.expires_at, now).to_string();
    api::SessionSummary {
        id: summary.id.to_string(),
        user_id: summary.user_id.to_string(),
        user_email: summary.user_email,
        status,
        created_at: summary.created_at,
        last_seen_at: summary.touched_at,
        expires_at: summary.expires_at,
        revoked_at: summary.revoked_at,
    }
}

/// Build an admin device-session summary using the specified timestamp for status evaluation.
#[must_use]
pub fn device_session_summary_at(
    summary: DbDeviceSessionSummary,
    now: DateTime<Utc>,
) -> api::DeviceSessionSummary {
    let status = session_status(summary.revoked_at, summary.expires_at, now).to_string();
    api::DeviceSessionSummary {
        id: summary.id.to_string(),
        user_id: summary.user_id.to_string(),
        user_email: summary.user_email,
        device_name: summary.device_name,
        platform: summary.platform.as_str().to_owned(),
        status,
        created_at: summary.created_at,
        last_used_at: summary.last_used_at,
        expires_at: summary.expires_at,
        revoked_at: summary.revoked_at,
    }
}

/// Compute session status from revocation and expiry fields.
fn session_status(
    revoked_at: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> &'static str {
    if revoked_at.is_some() {
        "revoked"
    } else if expires_at <= now {
        "expired"
    } else {
        "active"
    }
}

/// Build an admin org summary from a database summary.
#[must_use]
pub fn org_summary(summary: DbOrgSummary) -> api::OrgSummary {
    api::OrgSummary {
        id: summary.id.to_string(),
        name: summary.name,
        display_name: summary.display_name,
        created_at: summary.created_at,
    }
}

/// Build an admin org detail from a database detail.
#[must_use]
pub fn org_detail(detail: DbOrgDetail) -> api::OrgDetail {
    api::OrgDetail {
        id: detail.id.to_string(),
        name: detail.name,
        display_name: detail.display_name,
        created_by: detail.created_by.map(|id| id.to_string()),
        namespace_id: detail.namespace_id.to_string(),
        namespace_status: detail.namespace_status,
        namespace_gen: detail.namespace_gen,
        created_at: detail.created_at,
        updated_at: detail.updated_at,
    }
}

/// Build an admin org member from a database member.
#[must_use]
pub fn org_member(member: DbOrgMember) -> api::OrgMember {
    api::OrgMember {
        user_id: member.user_id.to_string(),
        username: member.username,
        email: member.email,
        role: member.role.as_str().to_owned(),
        created_at: member.created_at,
    }
}

/// Build an admin org invite from a database invite.
#[must_use]
pub fn org_invite(invite: DbOrgInvite) -> api::OrgInvite {
    api::OrgInvite {
        id: invite.id.to_string(),
        email: invite.email,
        created_at: invite.created_at,
        expires_at: invite.expires_at,
        accepted_at: invite.accepted_at,
        revoked_at: invite.revoked_at,
    }
}

/// Build an admin settings response from database app settings.
#[must_use]
pub const fn settings_response(settings: AppSettings) -> api::SettingsResponse {
    api::SettingsResponse {
        waitlist_enabled: settings.waitlist_enabled,
    }
}
