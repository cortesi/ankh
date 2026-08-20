#![warn(missing_docs)]

//! Deterministic identity fixtures and shared test harness helpers.

use std::{io, path::Path, time::Duration};

use ankh_constants::{DEFAULT_SESSION_TTL, ORG_INVITE_TTL};
use ankh_db::{AnkhDb, AnkhDbPool, Error as DbError, OrgId, Result as DbResult, UserId};
use ankh_types::OrgRole;
use chrono::{DateTime, TimeZone, Utc};
use tempfile::TempDir;
use uuid::Uuid;

/// Static user fixture used by shared identity tests.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UserFixture {
    /// Username / namespace name.
    pub username: &'static str,
    /// Email address.
    pub email: &'static str,
    /// Plain-text password used by deterministic tests.
    pub password: &'static str,
    /// Whether the email starts verified.
    pub verified: bool,
}

/// Static sysadmin fixture used by shared admin tests.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SysadminFixture {
    /// Email address.
    pub email: &'static str,
    /// Plain-text password used by deterministic tests.
    pub password: &'static str,
}

/// Static organization fixture used by shared org tests.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OrgFixture {
    /// Organization namespace name.
    pub name: &'static str,
    /// Optional display name.
    pub display_name: Option<&'static str>,
    /// Owner user email.
    pub owner_email: &'static str,
    /// Members as `(email, role)` entries; owner is not repeated here.
    pub members: &'static [(&'static str, OrgRole)],
}

/// Static organization invite fixture used by shared invite tests.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OrgInviteFixture {
    /// Organization namespace name.
    pub org_name: &'static str,
    /// Invited email address.
    pub email: &'static str,
    /// Role granted after invite acceptance.
    pub role: OrgRole,
    /// Deterministic raw invite token.
    pub token: &'static str,
}

/// Static mail fixture used by shared mail tests.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MailFixture {
    /// Sender address.
    pub sender: &'static str,
    /// Support address.
    pub support: &'static str,
    /// Public base URL used in generated links.
    pub public_base_url: &'static str,
}

/// Primary verified user fixture.
pub const ALICE: UserFixture = UserFixture {
    username: "alice",
    email: "alice@example.com",
    password: "al1ce-pass",
    verified: true,
};

/// Secondary unverified user fixture.
pub const BOB: UserFixture = UserFixture {
    username: "bob",
    email: "bob@example.com",
    password: "b0b-pass",
    verified: false,
};

/// Web-session token seeded for [`ALICE`].
pub const ALICE_SESSION_TOKEN: &str = "11111111-1111-1111-1111-111111111111";

/// Web-session token seeded for [`BOB`].
pub const BOB_SESSION_TOKEN: &str = "22222222-2222-2222-2222-222222222222";

/// Default session expiry used for seeded accounts.
pub const SESSION_TTL: Duration = DEFAULT_SESSION_TTL;

/// Primary sysadmin fixture.
pub const ADMIN: SysadminFixture = SysadminFixture {
    email: "admin@example.com",
    password: "password123",
};

/// Default organization fixture.
pub const DEFAULT_ORG: OrgFixture = OrgFixture {
    name: "test-org",
    display_name: Some("Test Organization"),
    owner_email: ALICE.email,
    members: &[(BOB.email, OrgRole::Member)],
};

/// Pending organization invite fixture.
pub const PENDING_ORG_INVITE: OrgInviteFixture = OrgInviteFixture {
    org_name: DEFAULT_ORG.name,
    email: "invited@example.com",
    role: OrgRole::Member,
    token: "org-invite-token-12345",
};

/// Shared mail branding fixture.
pub const MAIL: MailFixture = MailFixture {
    sender: "no-reply@example.com",
    support: "support@example.com",
    public_base_url: "http://127.0.0.1:52700",
};

/// IDs created by [`seed_identity_rows`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SeededIdentityIds {
    /// Alice's user ID.
    pub alice_user_id: UserId,
    /// Bob's user ID.
    pub bob_user_id: UserId,
    /// Default organization ID.
    pub default_org_id: OrgId,
}

/// Seed deterministic shared identity rows through an Ankh pool.
pub async fn seed_identities(pool: AnkhDbPool) -> DbResult<()> {
    let mut db = pool.get().await?;
    seed_identity_rows(&mut db).await?;
    Ok(())
}

/// Seed deterministic shared identity rows on an already checked-out handle.
pub async fn seed_identity_rows(db: &mut AnkhDb) -> DbResult<SeededIdentityIds> {
    clear_identity_rows(db).await?;
    seed_users_and_sessions(db).await?;
    seed_sysadmin(db).await?;
    let default_org_id = seed_default_org(db).await?;
    seed_pending_org_invite(db, default_org_id).await?;

    Ok(SeededIdentityIds {
        alice_user_id: db.get_user_by_email(ALICE.email).await?.id,
        bob_user_id: db.get_user_by_email(BOB.email).await?.id,
        default_org_id,
    })
}

/// Remove the known seeded identity rows so seeding is repeatable.
async fn clear_identity_rows(db: &AnkhDb) -> DbResult<()> {
    drop_org_if_exists(db, DEFAULT_ORG.name).await?;
    drop_session_if_exists(db, ALICE_SESSION_TOKEN).await?;
    drop_session_if_exists(db, BOB_SESSION_TOKEN).await?;
    drop_user_if_exists(db, ALICE.email).await?;
    drop_user_if_exists(db, BOB.email).await?;
    Ok(())
}

/// Seed deterministic users and their long-lived sessions.
async fn seed_users_and_sessions(db: &mut AnkhDb) -> DbResult<()> {
    for user in [ALICE, BOB] {
        db.add_user(user.username, user.email, user.password)
            .await?;
        if user.verified {
            db.mark_email_verified(user.email).await?;
        }
    }
    db.add_session(ALICE_SESSION_TOKEN, ALICE.email, SESSION_TTL)
        .await?;
    db.add_session(BOB_SESSION_TOKEN, BOB.email, SESSION_TTL)
        .await?;
    Ok(())
}

/// Seed the shared sysadmin account, tolerating an existing row.
async fn seed_sysadmin(db: &AnkhDb) -> DbResult<()> {
    match db.add_sysadmin(ADMIN.email, ADMIN.password).await {
        Ok(_) | Err(DbError::SysadminExists(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Seed the default organization and members.
async fn seed_default_org(db: &mut AnkhDb) -> DbResult<OrgId> {
    let owner = db.get_user_by_email(DEFAULT_ORG.owner_email).await?;
    let org_id = db
        .create_org(DEFAULT_ORG.name, DEFAULT_ORG.display_name, owner.id)
        .await?;
    for (member_email, role) in DEFAULT_ORG.members {
        let member = db.get_user_by_email(member_email).await?;
        db.add_org_member(org_id, member.id, *role, Some(owner.id))
            .await?;
    }
    Ok(org_id)
}

/// Seed the deterministic pending organization invite.
async fn seed_pending_org_invite(db: &AnkhDb, org_id: OrgId) -> DbResult<()> {
    debug_assert_eq!(PENDING_ORG_INVITE.role, OrgRole::Member);
    let owner = db.get_org_owner(org_id).await?;
    db.create_org_invite_with_token(
        org_id,
        PENDING_ORG_INVITE.email,
        owner.user_id,
        ORG_INVITE_TTL,
        PENDING_ORG_INVITE.token,
    )
    .await?;
    Ok(())
}

/// Remove an organization fixture if present.
async fn drop_org_if_exists(db: &AnkhDb, name: &str) -> DbResult<()> {
    let org = match db.get_org_by_name(name).await {
        Ok(org) => org,
        Err(DbError::OrgMissing(_)) => return Ok(()),
        Err(error) => return Err(error),
    };

    for invite in db.list_org_invites(org.id).await? {
        drop(db.cancel_org_invite(invite.id).await);
    }
    for member in db.list_org_members(org.id).await? {
        if member.role != OrgRole::Owner {
            drop(db.remove_org_member(org.id, member.user_id).await);
        }
    }
    match db.delete_org(org.id).await {
        Ok(()) | Err(DbError::OrgMissing(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Remove a session fixture if present.
async fn drop_session_if_exists(db: &AnkhDb, session_token: &str) -> DbResult<()> {
    match db.delete_session(session_token).await {
        Ok(()) | Err(DbError::SessionMissing(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Remove a user fixture if present.
async fn drop_user_if_exists(db: &AnkhDb, email: &str) -> DbResult<()> {
    match db.delete_user(email).await {
        Ok(()) | Err(DbError::UserMissing(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Fixed clock used by deterministic tests.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FixedClock {
    /// Timestamp returned by the fixed clock.
    now: DateTime<Utc>,
}

impl Default for FixedClock {
    fn default() -> Self {
        Self {
            now: Utc
                .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
                .single()
                .expect("fixed timestamp is valid"),
        }
    }
}

impl FixedClock {
    /// Return the current fixed timestamp.
    #[must_use]
    pub const fn now(self) -> DateTime<Utc> {
        self.now
    }
}

/// Deterministic UUID generator used by scaffold tests and future harnesses.
#[derive(Debug, Clone)]
pub struct FixedIdGenerator {
    /// Remaining IDs returned by the generator.
    ids: Vec<Uuid>,
}

impl FixedIdGenerator {
    /// Create a generator from IDs that are returned in the provided order.
    #[must_use]
    pub fn new(ids: Vec<Uuid>) -> Self {
        Self { ids }
    }

    /// Return the next deterministic ID.
    #[must_use]
    pub fn next_id(&mut self) -> Option<Uuid> {
        if self.ids.is_empty() {
            None
        } else {
            Some(self.ids.remove(0))
        }
    }
}

/// Temporary directory helper reserved for Ankh harnesses.
#[derive(Debug)]
pub struct TempDirHelper {
    /// Owned temporary directory removed when the helper is dropped.
    temp_dir: TempDir,
}

impl TempDirHelper {
    /// Create a fresh temporary directory.
    pub fn new() -> io::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        Ok(Self { temp_dir })
    }

    /// Return the temporary directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }
}

#[cfg(test)]
mod tests {
    //! Smoke tests for deterministic harness helpers.

    use std::future::Future;

    use ankh_constants::{
        DEFAULT_SYSADMIN_TOKEN_TTL, DEVICE_AUTH_GRANT_TTL, DEVICE_SESSION_TTL, PASSWORD_RESET_TTL,
    };
    use ankh_db::{
        DeviceAuthGrantRequest, DevicePlatform, Error as DbError, Result as DbResult, TokenKind,
        test_support::with_fresh_db,
    };
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use sha2::{Digest, Sha256};
    use tokio::runtime::Builder as TokioRuntimeBuilder;
    use uuid::Uuid;

    use super::{
        ADMIN, ALICE, ALICE_SESSION_TOKEN, BOB, DEFAULT_ORG, FixedClock, FixedIdGenerator,
        PENDING_ORG_INVITE, SESSION_TTL, TempDirHelper, seed_identities,
    };

    /// Run an async future to completion on a fresh current-thread runtime.
    fn run_async<T>(future: impl Future<Output = T>) -> T {
        TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .expect("create tokio runtime")
            .block_on(future)
    }

    /// Compute the PKCE S256 challenge for a verifier.
    fn pkce_challenge(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(hasher.finalize())
    }

    /// Proves static identity fixtures expose the expected accounts.
    #[test]
    fn static_identity_fixtures_are_stable() {
        assert_eq!(ALICE.email, "alice@example.com");
        assert_eq!(ADMIN.email, "admin@example.com");
        assert_eq!(DEFAULT_ORG.owner_email, ALICE.email);
        assert_eq!(DEFAULT_ORG.name, "test-org");
        assert_eq!(PENDING_ORG_INVITE.email, "invited@example.com");
        assert_eq!(ALICE_SESSION_TOKEN, "11111111-1111-1111-1111-111111111111");
        assert_eq!(SESSION_TTL.as_secs(), 60 * 60 * 24 * 30);
    }

    /// Proves the shared seed helper covers the live Ankh DB identity contract.
    #[test]
    fn seeded_identity_db_contract() {
        run_async(async {
            with_fresh_db(seed_identities, |fresh| async move {
                let mut db = fresh.get().await?;
                let alice = db.get_user_by_email(ALICE.email).await?;
                let bob = db.get_user_by_email(BOB.email).await?;

                assert!(db.is_email_verified(ALICE.email).await?);
                assert!(!db.is_email_verified(BOB.email).await?);
                assert_eq!(
                    db.get_session(ALICE_SESSION_TOKEN).await?.email,
                    ALICE.email
                );

                let session_token = db.signin(ALICE.email, ALICE.password, SESSION_TTL).await?;
                let touched = db.touch_session(session_token.as_str()).await?;
                assert_eq!(touched.email, ALICE.email);

                let reset_token = db
                    .create_token(ALICE.email, TokenKind::PasswordReset, PASSWORD_RESET_TTL)
                    .await?;
                assert_eq!(
                    db.peek_token(reset_token.as_str(), TokenKind::PasswordReset)
                        .await?,
                    ALICE.email
                );
                assert_eq!(
                    db.consume_token(reset_token.as_str(), TokenKind::PasswordReset)
                        .await?,
                    ALICE.email
                );

                let org = db.get_org_by_name(DEFAULT_ORG.name).await?;
                let bob_member = db.get_org_member(org.id, bob.id).await?;
                assert_eq!(bob_member.role, ankh_types::OrgRole::Member);
                assert_eq!(
                    db.get_org_invite(PENDING_ORG_INVITE.token).await?.email,
                    PENDING_ORG_INVITE.email
                );

                let status = db.set_namespace_suspended(org.namespace_id, true).await?;
                assert_eq!(status.name, DEFAULT_ORG.name);
                assert!(status.suspended);
                assert!(status.r#gen > 0);

                let (sysadmin_token, sysadmin) = db
                    .sysadmin_login(ADMIN.email, ADMIN.password, DEFAULT_SYSADMIN_TOKEN_TTL)
                    .await?;
                let validated = db.validate_sysadmin_token(sysadmin_token.as_str()).await?;
                assert_eq!(validated.id, sysadmin.id);

                let verifier = "test-device-verifier";
                let challenge = pkce_challenge(verifier);
                let created_grant = db
                    .create_device_auth_grant(DeviceAuthGrantRequest {
                        user_id: alice.id,
                        code_challenge: challenge.as_str(),
                        state: "state-123",
                        redirect_port: 52_700,
                        device_name: "Desktop",
                        platform: DevicePlatform::Macos,
                        ttl: DEVICE_AUTH_GRANT_TTL,
                    })
                    .await?;
                let grant = db
                    .consume_device_auth_grant(created_grant.code.as_str(), verifier)
                    .await?;
                assert_eq!(grant.user_id, alice.id);

                let created_session = db
                    .create_device_session(
                        alice.id,
                        "Desktop",
                        &DevicePlatform::Macos,
                        DEVICE_SESSION_TTL,
                    )
                    .await?;
                let validated_session = db
                    .validate_device_session(created_session.token.as_str())
                    .await?;
                assert_eq!(validated_session.id, created_session.session.id);
                db.revoke_device_session(created_session.session.id, alice.id)
                    .await?;
                assert!(matches!(
                    db.validate_device_session(created_session.token.as_str())
                        .await,
                    Err(DbError::DeviceSessionRevoked(_))
                ));

                DbResult::Ok(())
            })
            .await
            .expect("seeded identity DB contract passes");
        });
    }

    /// Proves the default clock is stable.
    #[test]
    fn fixed_clock_is_stable() {
        assert_eq!(FixedClock::default().now().timestamp(), 1_767_225_600);
    }

    /// Proves the ID generator yields configured IDs once.
    #[test]
    fn fixed_id_generator_is_ordered() {
        let id = Uuid::from_u128(1);
        let mut generator = FixedIdGenerator::new(vec![id]);

        assert_eq!(generator.next_id(), Some(id));
        assert_eq!(generator.next_id(), None);
    }

    /// Proves temp directories are created for harness use.
    #[test]
    fn temp_dir_helper_has_existing_path() {
        let helper = TempDirHelper::new().expect("temp dir can be created");

        assert!(helper.path().exists());
    }
}
