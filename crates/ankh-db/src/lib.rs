#![warn(missing_docs)]

//! Canonical Ankh identity database schema and query layer.

mod error;
mod models;
mod pg;
mod pool;
mod schema;
mod support;
pub mod test_support;

use std::{result, time::Duration};

use ankh_names::NamePolicy;
pub use ankh_types::{
    DeviceAuthGrantId, DevicePlatform, DeviceSessionId, NamespaceId, NamespaceKind, OrgId,
    OrgInviteId, OrgRole, SessionId, SysadminId, UserId,
};
use chrono::{DateTime, Utc};
use deadpool_postgres::Object;
pub use error::{Error, Result};
pub use models::{
    AppSettings, CreatedDeviceAuthGrant, CreatedDeviceSession, DeviceAuthGrant,
    DeviceAuthGrantRequest, DeviceSession, DeviceSessionStatus, DeviceSessionSummary,
    NamespaceStatusUpdate, OrgDetail, OrgInvite, OrgMember, OrgSummary, Session, SessionStatus,
    SessionSummary, SysadminInfo, TokenKind, UserDetail, UserSummary,
};
pub use pool::{
    AnkhDbPool, create_pg_pool, create_pg_pool_with_max_size,
    create_pg_pool_with_max_size_and_config,
};
pub use schema::{ANKH_SCHEMA_VERSION, schema_sql};
pub use support::{ParsedCursor, PasswordHashing, hash_secret, make_cursor};
pub(crate) use support::{waitlist_status_from_db, waitlist_status_to_db};
use tokio_postgres::{Client, Row};

/// Configuration attached to an Ankh database handle.
#[derive(Clone)]
pub struct AnkhDbConfig {
    /// Password hashing configuration for newly created credentials.
    pub password_hashing: PasswordHashing,
    /// Namespace validation policy for user and organization names.
    pub name_policy: NamePolicy,
}

impl Default for AnkhDbConfig {
    fn default() -> Self {
        Self {
            password_hashing: PasswordHashing::production(),
            name_policy: NamePolicy::shared(),
        }
    }
}

/// Concrete Postgres-backed Ankh identity database handle.
pub struct AnkhDb {
    /// Checked-out Postgres client used by Ankh identity operations.
    client: Object,
    /// Runtime DB configuration.
    config: AnkhDbConfig,
}

impl AnkhDb {
    /// Build an Ankh DB handle from a checked-out pool object.
    #[must_use]
    pub fn new(client: Object) -> Self {
        Self::with_config(client, AnkhDbConfig::default())
    }

    /// Build an Ankh DB handle from a checked-out pool object and explicit config.
    #[must_use]
    pub fn with_config(client: Object, config: AnkhDbConfig) -> Self {
        Self { client, config }
    }

    /// Return the underlying checked-out Postgres client for colocated product SQL.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Return the mutable underlying checked-out Postgres client for colocated product SQL.
    pub fn client_mut(&mut self) -> &mut Client {
        &mut self.client
    }

    /// Validate a namespace name with this handle's configured name policy.
    fn validate_namespace_name(&self, name: &str) -> result::Result<(), &'static str> {
        self.config.name_policy.validate_namespace_name(name)
    }

    /// Validate only the shared namespace syntax, allowing reserved names.
    fn validate_name_format(&self, name: &str) -> result::Result<(), &'static str> {
        ankh_names::validate_name_format(name)
    }

    /// Hash a password using the configured hashing parameters.
    fn hash_password(&self, password: &str) -> Result<String> {
        self.config.password_hashing.hash_password(password)
    }

    /// Inserts a session row for an existing user using a caller-supplied identifier.
    async fn insert_session_for_user(
        &self,
        session_id: &str,
        email: &str,
        ttl: Duration,
    ) -> Result<()> {
        let user_exists = self
            .client
            .query_opt("SELECT 1 FROM users WHERE email = $1", &[&email])
            .await?
            .is_some();

        if !user_exists {
            return Err(Error::UserMissing(email.to_owned()));
        }

        let session_hash = hash_secret(session_id);
        let ttl_seconds: i64 = ttl.as_secs().try_into().unwrap_or(i64::MAX);
        let inserted = self
            .client
            .execute(
                "INSERT INTO sessions (token_hash, email, expires_at)
                 VALUES ($1, $2, CURRENT_TIMESTAMP + ($3::BIGINT * INTERVAL '1 second'))
                 ON CONFLICT (token_hash) DO NOTHING",
                &[&session_hash, &email, &ttl_seconds],
            )
            .await?;

        if inserted == 0 {
            return Err(Error::SessionExists(session_hash));
        }

        Ok(())
    }

    /// Converts a session query row into a `Session`.
    fn session_from_row(row: &Row) -> Session {
        Session {
            email: row.get(0),
            created_at: row.get(1),
            touched_at: row.get(2),
            expires_at: row.get(3),
        }
    }

    /// Apply the canonical Ankh identity schema to this database.
    pub async fn apply_schema(&self) -> Result<()> {
        self.client.batch_execute(schema_sql()).await?;
        Ok(())
    }

    /// Apply the canonical Ankh identity schema to a raw Postgres client.
    pub async fn apply_schema_to_client(client: &Client) -> Result<()> {
        client.batch_execute(schema_sql()).await?;
        Ok(())
    }

    /// Initialize Ankh identity metadata on a raw Postgres client.
    pub async fn initialize_client(client: &Client) -> Result<()> {
        client
            .execute(
                "INSERT INTO ankh_schema_version (version) VALUES ($1)
                 ON CONFLICT (version) DO NOTHING",
                &[&ANKH_SCHEMA_VERSION],
            )
            .await?;
        Self::ensure_settings_row_on_client(client).await
    }

    /// Ensure the singleton Ankh settings row exists on a raw Postgres client.
    pub async fn ensure_settings_row_on_client(client: &Client) -> Result<()> {
        client
            .execute(
                "INSERT INTO ankh_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING",
                &[],
            )
            .await?;
        Ok(())
    }

    /// Apply schema and initialize the database for Ankh identity operations.
    pub async fn bootstrap(&self) -> Result<()> {
        self.apply_schema().await?;
        self.initialize().await?;
        Ok(())
    }

    /// Record the current schema version and ensure singleton settings exist.
    pub async fn initialize(&self) -> Result<()> {
        Self::initialize_client(&self.client).await
    }

    /// Read the newest applied Ankh schema version.
    pub async fn version(&self) -> Result<Option<i32>> {
        let row = self
            .client
            .query_opt(
                "SELECT version FROM ankh_schema_version ORDER BY version DESC LIMIT 1",
                &[],
            )
            .await?;
        Ok(row.map(|row| row.get(0)))
    }

    /// Ensure the singleton Ankh settings row exists.
    pub async fn ensure_settings_row(&self) -> Result<()> {
        Self::ensure_settings_row_on_client(&self.client).await
    }

    /// Return the global identity settings.
    pub async fn get_app_settings(&self) -> Result<AppSettings> {
        pg::settings::get_app_settings(self).await
    }

    /// Update the waitlist setting.
    pub async fn set_waitlist_enabled(&self, enabled: bool) -> Result<AppSettings> {
        pg::settings::set_waitlist_enabled(self, enabled).await
    }

    /// Add a user with a freshly hashed password.
    pub async fn add_user(&mut self, username: &str, email: &str, password: &str) -> Result<()> {
        pg::user::add_user(self, username, email, password).await
    }

    /// Get a user by username.
    pub async fn get_user_by_name(&self, username: &str) -> Result<UserDetail> {
        pg::user::get_user_by_name(self, username).await
    }

    /// Get a user by email address.
    pub async fn get_user_by_email(&self, email: &str) -> Result<UserDetail> {
        pg::user::get_user_by_email(self, email).await
    }

    /// Delete a user by email address.
    pub async fn delete_user(&self, email: &str) -> Result<()> {
        pg::user::delete_user(self, email).await
    }

    /// Set a user's password.
    pub async fn set_password(&self, email: &str, password: &str) -> Result<()> {
        pg::user::set_password(self, email, password).await
    }

    /// Return whether a user's email has been verified.
    pub async fn is_email_verified(&self, email: &str) -> Result<bool> {
        pg::user::is_email_verified(self, email).await
    }

    /// Mark a user's email verified.
    pub async fn mark_email_verified(&self, email: &str) -> Result<()> {
        pg::user::mark_email_verified(self, email).await
    }

    /// Return whether a user is waitlisted.
    pub async fn is_user_waitlisted(&self, email: &str) -> Result<bool> {
        pg::user::is_user_waitlisted(self, email).await
    }

    /// Set whether a user is waitlisted.
    pub async fn set_user_waitlisted(&self, email: &str, waitlisted: bool) -> Result<()> {
        pg::user::set_user_waitlisted(self, email, waitlisted).await
    }

    /// Get a user by ID.
    pub async fn get_user_by_id(&self, id: UserId) -> Result<UserDetail> {
        pg::user::get_user_by_id(self, id).await
    }

    /// List users for admin views.
    pub async fn list_users(
        &self,
        limit: i64,
        cursor: Option<&str>,
        email_filter: Option<&str>,
    ) -> Result<(Vec<UserSummary>, Option<String>)> {
        pg::user::list_users(self, limit, cursor, email_filter).await
    }

    /// Delete a user by ID.
    pub async fn delete_user_by_id(&self, id: UserId) -> Result<()> {
        pg::user::delete_user_by_id(self, id).await
    }

    /// Sign in and create a web session.
    pub async fn signin(&self, identifier: &str, password: &str, ttl: Duration) -> Result<String> {
        pg::session::signin(self, identifier, password, ttl).await
    }

    /// Add a web session with a caller-provided token.
    pub async fn add_session(&self, session_id: &str, email: &str, ttl: Duration) -> Result<()> {
        pg::session::add_session(self, session_id, email, ttl).await
    }

    /// Get a web session.
    pub async fn get_session(&self, session_id: &str) -> Result<Session> {
        pg::session::get_session(self, session_id).await
    }

    /// Touch a web session.
    pub async fn touch_session(&self, session_id: &str) -> Result<Session> {
        pg::session::touch_session(self, session_id).await
    }

    /// Touch a web session only if it is stale.
    pub async fn touch_session_if_stale(
        &mut self,
        session_id: &str,
        stale_after: Duration,
    ) -> Result<Session> {
        pg::session::touch_session_if_stale(self, session_id, stale_after).await
    }

    /// Delete a web session.
    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        pg::session::delete_session(self, session_id).await
    }

    /// Delete all web sessions for a user email.
    pub async fn delete_sessions_for_email(&self, email: &str) -> Result<u64> {
        pg::session::delete_sessions_for_email(self, email).await
    }

    /// Delete expired web sessions.
    pub async fn delete_expired_sessions(&self) -> Result<u64> {
        pg::session::delete_expired_sessions(self).await
    }

    /// List web sessions for admin views.
    pub async fn list_sessions(
        &self,
        limit: i64,
        cursor: Option<&str>,
        user_id: Option<UserId>,
        status: Option<SessionStatus>,
    ) -> Result<(Vec<SessionSummary>, Option<String>)> {
        pg::session::list_sessions(self, limit, cursor, user_id, status).await
    }

    /// Revoke a web session by ID.
    pub async fn revoke_session_by_id(&self, id: SessionId) -> Result<()> {
        pg::session::revoke_session_by_id(self, id).await
    }

    /// Create a one-time identity token.
    pub async fn create_token(
        &self,
        email: &str,
        kind: TokenKind,
        ttl: Duration,
    ) -> Result<String> {
        pg::token::create_token(self, email, kind, ttl).await
    }

    /// Consume a one-time identity token.
    pub async fn consume_token(&mut self, token: &str, kind: TokenKind) -> Result<String> {
        pg::token::consume_token(self, token, kind).await
    }

    /// Peek a one-time identity token.
    pub async fn peek_token(&self, token: &str, kind: TokenKind) -> Result<String> {
        pg::token::peek_token(self, token, kind).await
    }

    /// Return the newest token creation timestamp for a user and kind.
    pub async fn latest_token_created_at(
        &self,
        email: &str,
        kind: TokenKind,
    ) -> Result<Option<DateTime<Utc>>> {
        pg::token::latest_token_created_at(self, email, kind).await
    }

    /// Delete tokens for a user email and kind.
    pub async fn delete_tokens_for_email(&self, email: &str, kind: TokenKind) -> Result<()> {
        pg::token::delete_tokens_for_email(self, email, kind).await
    }

    /// Delete expired one-time identity tokens.
    pub async fn delete_expired_tokens(&self) -> Result<u64> {
        pg::token::delete_expired_tokens(self).await
    }

    /// Create an account invite.
    pub async fn create_invite(&self, email: &str, ttl: Duration) -> Result<String> {
        pg::invite::create_invite(self, email, ttl).await
    }

    /// Consume an account invite.
    pub async fn consume_invite(&self, token: &str) -> Result<String> {
        pg::invite::consume_invite(self, token).await
    }

    /// Peek an account invite.
    pub async fn peek_invite(&self, token: &str) -> Result<String> {
        pg::invite::peek_invite(self, token).await
    }

    /// Delete account invites for an email address.
    pub async fn delete_invites_for_email(&self, email: &str) -> Result<()> {
        pg::invite::delete_invites_for_email(self, email).await
    }

    /// Create a sysadmin.
    pub async fn add_sysadmin(&self, email: &str, password: &str) -> Result<SysadminId> {
        pg::sysadmin::add_sysadmin(self, email, password).await
    }

    /// Sign in a sysadmin.
    pub async fn sysadmin_login(
        &self,
        email: &str,
        password: &str,
        ttl: Duration,
    ) -> Result<(String, SysadminInfo)> {
        pg::sysadmin::sysadmin_login(self, email, password, ttl).await
    }

    /// Validate a sysadmin bearer token.
    pub async fn validate_sysadmin_token(&self, token: &str) -> Result<SysadminInfo> {
        pg::sysadmin::validate_sysadmin_token(self, token).await
    }

    /// Revoke a sysadmin bearer token.
    pub async fn revoke_sysadmin_token(&self, token: &str) -> Result<()> {
        pg::sysadmin::revoke_sysadmin_token(self, token).await
    }

    /// List sysadmins for admin views.
    pub async fn list_sysadmins(
        &self,
        limit: i64,
        cursor: Option<&str>,
    ) -> Result<(Vec<SysadminInfo>, Option<String>)> {
        pg::sysadmin::list_sysadmins(self, limit, cursor).await
    }

    /// Get a sysadmin by ID.
    pub async fn get_sysadmin(&self, id: SysadminId) -> Result<SysadminInfo> {
        pg::sysadmin::get_sysadmin(self, id).await
    }

    /// Delete expired sysadmin bearer tokens.
    pub async fn delete_expired_sysadmin_tokens(&self) -> Result<u64> {
        pg::sysadmin::delete_expired_sysadmin_tokens(self).await
    }

    /// Create an organization.
    pub async fn create_org(
        &mut self,
        name: &str,
        display_name: Option<&str>,
        created_by: UserId,
    ) -> Result<OrgId> {
        pg::org::create_org(self, name, display_name, created_by).await
    }

    /// Create an organization while allowing reserved namespace words.
    pub async fn create_org_unchecked(
        &mut self,
        name: &str,
        display_name: Option<&str>,
        created_by: UserId,
    ) -> Result<OrgId> {
        pg::org::create_org_unchecked(self, name, display_name, created_by).await
    }

    /// Get an organization by ID.
    pub async fn get_org_by_id(&self, id: OrgId) -> Result<OrgDetail> {
        pg::org::get_org_by_id(self, id).await
    }

    /// Get an organization by name.
    pub async fn get_org_by_name(&self, name: &str) -> Result<OrgDetail> {
        pg::org::get_org_by_name(self, name).await
    }

    /// List organizations for a user.
    pub async fn list_orgs_for_user(&self, user_id: UserId) -> Result<Vec<OrgSummary>> {
        pg::org::list_orgs_for_user(self, user_id).await
    }

    /// List organizations for admin views.
    pub async fn list_all_orgs(
        &self,
        limit: i64,
        cursor: Option<&str>,
    ) -> Result<(Vec<OrgSummary>, Option<String>)> {
        pg::org::list_all_orgs(self, limit, cursor).await
    }

    /// Update an organization.
    pub async fn update_org(&self, id: OrgId, display_name: Option<&str>) -> Result<()> {
        pg::org::update_org(self, id, display_name).await
    }

    /// Delete an organization.
    pub async fn delete_org(&self, id: OrgId) -> Result<()> {
        pg::org::delete_org(self, id).await
    }

    /// Set namespace suspension state and bump its edge-visible generation.
    pub async fn set_namespace_suspended(
        &self,
        namespace_id: NamespaceId,
        suspended: bool,
    ) -> Result<NamespaceStatusUpdate> {
        pg::namespace::set_namespace_suspended(self, namespace_id, suspended).await
    }

    /// Return whether an organization has no non-owner members.
    pub async fn is_org_empty(&self, id: OrgId) -> Result<bool> {
        pg::org::is_org_empty(self, id).await
    }

    /// Get an organization member.
    pub async fn get_org_member(&self, org_id: OrgId, user_id: UserId) -> Result<OrgMember> {
        pg::org::get_org_member(self, org_id, user_id).await
    }

    /// List organization members.
    pub async fn list_org_members(&self, org_id: OrgId) -> Result<Vec<OrgMember>> {
        pg::org::list_org_members(self, org_id).await
    }

    /// Add an organization member.
    pub async fn add_org_member(
        &self,
        org_id: OrgId,
        user_id: UserId,
        role: OrgRole,
        added_by: Option<UserId>,
    ) -> Result<()> {
        pg::org::add_org_member(self, org_id, user_id, role, added_by).await
    }

    /// Remove an organization member.
    pub async fn remove_org_member(&self, org_id: OrgId, user_id: UserId) -> Result<()> {
        pg::org::remove_org_member(self, org_id, user_id).await
    }

    /// Change an organization member role.
    pub async fn set_org_member_role(
        &self,
        org_id: OrgId,
        user_id: UserId,
        role: OrgRole,
    ) -> Result<()> {
        pg::org::set_org_member_role(self, org_id, user_id, role).await
    }

    /// Transfer organization ownership.
    pub async fn transfer_org_ownership(&self, org_id: OrgId, new_owner_id: UserId) -> Result<()> {
        pg::org::transfer_org_ownership(self, org_id, new_owner_id).await
    }

    /// Get the owner member for an organization.
    pub async fn get_org_owner(&self, org_id: OrgId) -> Result<OrgMember> {
        pg::org::get_org_owner(self, org_id).await
    }

    /// Create an organization invite.
    pub async fn create_org_invite(
        &self,
        org_id: OrgId,
        email: &str,
        invited_by: UserId,
        ttl: Duration,
    ) -> Result<(String, OrgInviteId)> {
        pg::org::create_org_invite(self, org_id, email, invited_by, ttl).await
    }

    /// Create an organization invite with a caller-supplied token.
    pub async fn create_org_invite_with_token(
        &self,
        org_id: OrgId,
        email: &str,
        invited_by: UserId,
        ttl: Duration,
        token: &str,
    ) -> Result<OrgInviteId> {
        pg::org::create_org_invite_with_token(self, org_id, email, invited_by, ttl, token).await
    }

    /// Get an organization invite by raw token.
    pub async fn get_org_invite(&self, token: &str) -> Result<OrgInvite> {
        pg::org::get_org_invite(self, token).await
    }

    /// Accept an organization invite.
    pub async fn accept_org_invite(&self, token: &str, user_id: UserId) -> Result<()> {
        pg::org::accept_org_invite(self, token, user_id).await
    }

    /// Cancel an organization invite.
    pub async fn cancel_org_invite(&self, invite_id: OrgInviteId) -> Result<()> {
        pg::org::cancel_org_invite(self, invite_id).await
    }

    /// List organization invites.
    pub async fn list_org_invites(&self, org_id: OrgId) -> Result<Vec<OrgInvite>> {
        pg::org::list_org_invites(self, org_id).await
    }

    /// Delete expired organization invites.
    pub async fn delete_expired_org_invites(&self) -> Result<u64> {
        pg::org::delete_expired_org_invites(self).await
    }

    /// Create a device authorization grant.
    pub async fn create_device_auth_grant(
        &self,
        request: DeviceAuthGrantRequest<'_>,
    ) -> Result<CreatedDeviceAuthGrant> {
        pg::device::create_device_auth_grant(self, request).await
    }

    /// Consume a device authorization grant.
    pub async fn consume_device_auth_grant(
        &mut self,
        code: &str,
        code_verifier: &str,
    ) -> Result<DeviceAuthGrant> {
        pg::device::consume_device_auth_grant(self, code, code_verifier).await
    }

    /// Create a device session.
    pub async fn create_device_session(
        &self,
        user_id: UserId,
        device_name: &str,
        platform: &DevicePlatform,
        ttl: Duration,
    ) -> Result<CreatedDeviceSession> {
        pg::device::create_device_session(self, user_id, device_name, platform, ttl).await
    }

    /// Validate a device session bearer token.
    pub async fn validate_device_session(&self, token: &str) -> Result<DeviceSession> {
        pg::device::validate_device_session(self, token).await
    }

    /// List active device sessions for a user.
    pub async fn list_device_sessions_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<DeviceSession>> {
        pg::device::list_device_sessions_for_user(self, user_id).await
    }

    /// Get a device session by ID.
    pub async fn get_device_session(&self, id: DeviceSessionId) -> Result<DeviceSession> {
        pg::device::get_device_session(self, id).await
    }

    /// Revoke a device session belonging to a user.
    pub async fn revoke_device_session(&self, id: DeviceSessionId, user_id: UserId) -> Result<()> {
        pg::device::revoke_device_session(self, id, user_id).await
    }

    /// Revoke all device sessions for a user.
    pub async fn revoke_all_device_sessions(&self, user_id: UserId) -> Result<u64> {
        pg::device::revoke_all_device_sessions(self, user_id).await
    }

    /// List device sessions for admin views.
    pub async fn list_device_sessions(
        &self,
        limit: i64,
        cursor: Option<&str>,
        user_id: Option<UserId>,
        status: Option<DeviceSessionStatus>,
    ) -> Result<(Vec<DeviceSessionSummary>, Option<String>)> {
        pg::device::list_device_sessions(self, limit, cursor, user_id, status).await
    }

    /// Revoke a device session by ID.
    pub async fn revoke_device_session_by_id(&self, id: DeviceSessionId) -> Result<()> {
        pg::device::revoke_device_session_by_id(self, id).await
    }
}

#[cfg(test)]
mod tests {
    //! Smoke tests for the schema foundation.

    use super::schema::{ANKH_SCHEMA_VERSION, schema_sql};

    /// Proves the schema has Ankh-owned table names rather than leaf aliases.
    #[test]
    fn schema_uses_ankh_names_for_shared_tables() {
        assert!(schema_sql().contains("CREATE TABLE IF NOT EXISTS ankh_schema_version"));
        assert!(schema_sql().contains("CREATE TABLE IF NOT EXISTS ankh_settings"));
        assert!(schema_sql().contains("CREATE TABLE IF NOT EXISTS device_auth_grants"));
        assert!(schema_sql().contains("CREATE TABLE IF NOT EXISTS device_sessions"));
        assert!(!schema_sql().contains("CREATE TABLE IF NOT EXISTS app_settings"));
        assert!(!schema_sql().contains("CREATE TABLE IF NOT EXISTS player_sessions"));
    }

    /// Proves the schema carries the Restless namespace superset columns.
    #[test]
    fn schema_keeps_namespace_superset_columns() {
        assert!(schema_sql().contains("tier TEXT NOT NULL DEFAULT 'free'"));
        assert!(schema_sql().contains("limits_override JSONB"));
        assert!(schema_sql().contains("status TEXT NOT NULL DEFAULT 'active'"));
        assert!(schema_sql().contains("gen BIGINT NOT NULL DEFAULT 0"));
    }

    /// Proves device platform storage is intentionally open text.
    #[test]
    fn device_platform_storage_is_open_text() {
        assert!(schema_sql().contains("platform TEXT NOT NULL"));
        assert!(!schema_sql().contains("platform IN"));
    }

    /// Proves the embedded schema is valid for `batch_execute` and not `psql`.
    #[test]
    fn schema_has_no_psql_meta_commands() {
        for line in schema_sql().lines() {
            assert!(
                !line.trim_start().starts_with('\\'),
                "schema contains psql meta-command: {line}"
            );
        }
    }

    /// Proves the version constant is ready for initialization.
    #[test]
    fn schema_version_starts_at_one() {
        assert_eq!(ANKH_SCHEMA_VERSION, 1);
    }
}
