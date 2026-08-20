//! Organization-related database operations.

use std::time::Duration;

use ankh_constants::ADMIN_LIST_MAX_LIMIT;
use ankh_names::normalize_name;
use chrono::Utc;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::{
    AnkhDb, Error, NamespaceId, NamespaceKind, OrgDetail, OrgId, OrgInvite, OrgInviteId, OrgMember,
    OrgRole, OrgSummary, Result, UserId, hash_secret,
};

/// Creates a new organization.
pub async fn create_org(
    db: &mut AnkhDb,
    name: &str,
    display_name: Option<&str>,
    created_by: UserId,
) -> Result<OrgId> {
    create_org_inner(db, name, display_name, created_by, true).await
}

/// Creates a new organization while skipping the reserved-name check.
pub async fn create_org_unchecked(
    db: &mut AnkhDb,
    name: &str,
    display_name: Option<&str>,
    created_by: UserId,
) -> Result<OrgId> {
    create_org_inner(db, name, display_name, created_by, false).await
}

/// Gets an organization by ID.
pub async fn get_org_by_id(db: &AnkhDb, id: OrgId) -> Result<OrgDetail> {
    let row = db
        .client
        .query_opt(
            "SELECT o.id, o.namespace_id, n.name, o.display_name, o.created_by, n.status, n.gen,
                o.created_at, o.updated_at
         FROM organizations o
         JOIN namespaces n ON n.id = o.namespace_id
         WHERE o.id = $1",
            &[&id.0],
        )
        .await?;

    match row {
        Some(row) => Ok(org_detail_from_row(&row)),
        None => Err(Error::OrgMissing(id.to_string())),
    }
}

/// Gets an organization by name (namespace name).
pub async fn get_org_by_name(db: &AnkhDb, name: &str) -> Result<OrgDetail> {
    let normalized = normalize_name(name);

    let row = db
        .client
        .query_opt(
            "SELECT o.id, o.namespace_id, n.name, o.display_name, o.created_by, n.status, n.gen,
                o.created_at, o.updated_at
         FROM organizations o
         JOIN namespaces n ON n.id = o.namespace_id
         WHERE n.name = $1",
            &[&normalized],
        )
        .await?;

    match row {
        Some(row) => Ok(org_detail_from_row(&row)),
        None => Err(Error::OrgMissing(normalized)),
    }
}

/// Lists organizations a user belongs to.
pub async fn list_orgs_for_user(db: &AnkhDb, user_id: UserId) -> Result<Vec<OrgSummary>> {
    let rows = db
        .client
        .query(
            "SELECT o.id, o.namespace_id, n.name, o.display_name, o.created_at
         FROM organizations o
         JOIN namespaces n ON n.id = o.namespace_id
         JOIN org_members m ON m.org_id = o.id
         WHERE m.user_id = $1
         ORDER BY o.created_at DESC",
            &[&user_id.0],
        )
        .await?;

    Ok(rows.iter().map(org_summary_from_row).collect())
}

/// Lists all organizations with optional pagination.
pub async fn list_all_orgs(
    db: &AnkhDb,
    limit: i64,
    cursor: Option<&str>,
) -> Result<(Vec<OrgSummary>, Option<String>)> {
    let limit = limit.clamp(1, ADMIN_LIST_MAX_LIMIT);

    let rows = if let Some(cursor) = cursor {
        let cursor_id: Uuid = cursor
            .parse()
            .map_err(|_| Error::OrgMissing("invalid cursor".to_string()))?;
        db.client
            .query(
                "SELECT o.id, o.namespace_id, n.name, o.display_name, o.created_at
             FROM organizations o
             JOIN namespaces n ON n.id = o.namespace_id
             WHERE o.id > $1
             ORDER BY o.id
             LIMIT $2",
                &[&cursor_id, &(limit + 1)],
            )
            .await?
    } else {
        db.client
            .query(
                "SELECT o.id, o.namespace_id, n.name, o.display_name, o.created_at
             FROM organizations o
             JOIN namespaces n ON n.id = o.namespace_id
             ORDER BY o.id
             LIMIT $1",
                &[&(limit + 1)],
            )
            .await?
    };

    let has_more = rows.len() as i64 > limit;
    let orgs: Vec<OrgSummary> = rows
        .iter()
        .take(limit as usize)
        .map(org_summary_from_row)
        .collect();

    let next_cursor = if has_more {
        orgs.last().map(|o| o.id.to_string())
    } else {
        None
    };

    Ok((orgs, next_cursor))
}

/// Updates an organization's display name.
pub async fn update_org(db: &AnkhDb, id: OrgId, display_name: Option<&str>) -> Result<()> {
    let updated = db
        .client
        .execute(
            "UPDATE organizations SET display_name = $1, updated_at = now() WHERE id = $2",
            &[&display_name, &id.0],
        )
        .await?;

    if updated == 0 {
        return Err(Error::OrgMissing(id.to_string()));
    }

    Ok(())
}

/// Deletes an organization.
pub async fn delete_org(db: &AnkhDb, id: OrgId) -> Result<()> {
    // Check if org is empty (only owner remains)
    if !is_org_empty(db, id).await? {
        return Err(Error::OrgNotEmpty(id.to_string()));
    }

    // Get the namespace_id before deleting
    let row = db
        .client
        .query_opt(
            "SELECT namespace_id FROM organizations WHERE id = $1",
            &[&id.0],
        )
        .await?;

    let namespace_id: Uuid = match row {
        Some(row) => row.get(0),
        None => return Err(Error::OrgMissing(id.to_string())),
    };

    // Delete org_members (will cascade from org delete, but be explicit)
    db.client
        .execute("DELETE FROM org_members WHERE org_id = $1", &[&id.0])
        .await?;

    // Delete org_invites (will cascade from org delete, but be explicit)
    db.client
        .execute("DELETE FROM org_invites WHERE org_id = $1", &[&id.0])
        .await?;

    // Delete organization
    db.client
        .execute("DELETE FROM organizations WHERE id = $1", &[&id.0])
        .await?;

    // Delete namespace
    db.client
        .execute("DELETE FROM namespaces WHERE id = $1", &[&namespace_id])
        .await?;

    Ok(())
}

/// Returns true if the organization has no members besides the owner.
pub async fn is_org_empty(db: &AnkhDb, id: OrgId) -> Result<bool> {
    let row = db
        .client
        .query_one(
            "SELECT COUNT(*) FROM org_members WHERE org_id = $1",
            &[&id.0],
        )
        .await?;

    let count: i64 = row.get(0);
    // Empty means only the owner (1 member)
    Ok(count <= 1)
}

/// Gets a member's info within an organization.
pub async fn get_org_member(db: &AnkhDb, org_id: OrgId, user_id: UserId) -> Result<OrgMember> {
    let row = db
        .client
        .query_opt(
            "SELECT m.user_id, n.name, u.email, m.role, m.added_by, m.created_at, m.updated_at
         FROM org_members m
         JOIN users u ON u.id = m.user_id
         JOIN namespaces n ON n.id = u.namespace_id
         WHERE m.org_id = $1 AND m.user_id = $2",
            &[&org_id.0, &user_id.0],
        )
        .await?;

    match row {
        Some(row) => Ok(org_member_from_row(&row)),
        None => Err(Error::NotOrgMember(user_id.to_string(), org_id.to_string())),
    }
}

/// Lists all members of an organization.
pub async fn list_org_members(db: &AnkhDb, org_id: OrgId) -> Result<Vec<OrgMember>> {
    let rows = db
        .client
        .query(
            "SELECT m.user_id, n.name, u.email, m.role, m.added_by, m.created_at, m.updated_at
         FROM org_members m
         JOIN users u ON u.id = m.user_id
         JOIN namespaces n ON n.id = u.namespace_id
         WHERE m.org_id = $1
         ORDER BY m.created_at ASC",
            &[&org_id.0],
        )
        .await?;

    Ok(rows.iter().map(org_member_from_row).collect())
}

/// Adds a user as a member of an organization.
pub async fn add_org_member(
    db: &AnkhDb,
    org_id: OrgId,
    user_id: UserId,
    role: OrgRole,
    added_by: Option<UserId>,
) -> Result<()> {
    // Cannot add as owner through this function
    if role == OrgRole::Owner {
        return Err(Error::PermissionDenied(
            "cannot add member as owner, use transfer_org_ownership".to_owned(),
        ));
    }

    let role_str = role.as_str();
    let added_by_id = added_by.map(|u| u.0);

    let inserted = db
        .client
        .execute(
            "INSERT INTO org_members (org_id, user_id, role, added_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (org_id, user_id) DO NOTHING",
            &[&org_id.0, &user_id.0, &role_str, &added_by_id],
        )
        .await?;

    if inserted == 0 {
        return Err(Error::AlreadyOrgMember(
            user_id.to_string(),
            org_id.to_string(),
        ));
    }

    Ok(())
}

/// Removes a member from an organization.
pub async fn remove_org_member(db: &AnkhDb, org_id: OrgId, user_id: UserId) -> Result<()> {
    // Check if user is owner
    let row = db
        .client
        .query_opt(
            "SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2",
            &[&org_id.0, &user_id.0],
        )
        .await?;

    match row {
        Some(row) => {
            let role_str: String = row.get(0);
            if let Some(OrgRole::Owner) = OrgRole::parse_db(&role_str) {
                return Err(Error::PermissionDenied(
                    "cannot remove owner, transfer ownership first".to_owned(),
                ));
            }
        }
        None => {
            return Err(Error::NotOrgMember(user_id.to_string(), org_id.to_string()));
        }
    }

    db.client
        .execute(
            "DELETE FROM org_members WHERE org_id = $1 AND user_id = $2",
            &[&org_id.0, &user_id.0],
        )
        .await?;

    Ok(())
}

/// Changes a member's role within an organization.
pub async fn set_org_member_role(
    db: &AnkhDb,
    org_id: OrgId,
    user_id: UserId,
    role: OrgRole,
) -> Result<()> {
    // Cannot set to Owner through this function
    if role == OrgRole::Owner {
        return Err(Error::PermissionDenied(
            "cannot set role to owner, use transfer_org_ownership".to_owned(),
        ));
    }

    // Check current role - cannot change owner's role
    let row = db
        .client
        .query_opt(
            "SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2",
            &[&org_id.0, &user_id.0],
        )
        .await?;

    match row {
        Some(row) => {
            let role_str: String = row.get(0);
            if let Some(OrgRole::Owner) = OrgRole::parse_db(&role_str) {
                return Err(Error::PermissionDenied(
                    "cannot change owner's role, transfer ownership first".to_owned(),
                ));
            }
        }
        None => {
            return Err(Error::NotOrgMember(user_id.to_string(), org_id.to_string()));
        }
    }

    let role_str = role.as_str();
    db.client
        .execute(
            "UPDATE org_members SET role = $1, updated_at = now()
         WHERE org_id = $2 AND user_id = $3",
            &[&role_str, &org_id.0, &user_id.0],
        )
        .await?;

    Ok(())
}

/// Transfers ownership of an organization to another member.
pub async fn transfer_org_ownership(
    db: &AnkhDb,
    org_id: OrgId,
    new_owner_id: UserId,
) -> Result<()> {
    // Check that new owner is already a member
    let row = db
        .client
        .query_opt(
            "SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2",
            &[&org_id.0, &new_owner_id.0],
        )
        .await?;

    if row.is_none() {
        return Err(Error::NotOrgMember(
            new_owner_id.to_string(),
            org_id.to_string(),
        ));
    }

    let owner_role = OrgRole::Owner.as_str();
    let admin_role = OrgRole::Admin.as_str();

    // Demote current owner to admin
    db.client
        .execute(
            "UPDATE org_members SET role = $1, updated_at = now()
         WHERE org_id = $2 AND role = $3",
            &[&admin_role, &org_id.0, &owner_role],
        )
        .await?;

    // Promote new owner
    db.client
        .execute(
            "UPDATE org_members SET role = $1, updated_at = now()
         WHERE org_id = $2 AND user_id = $3",
            &[&owner_role, &org_id.0, &new_owner_id.0],
        )
        .await?;

    Ok(())
}

/// Gets the owner of an organization.
pub async fn get_org_owner(db: &AnkhDb, org_id: OrgId) -> Result<OrgMember> {
    let owner_role = OrgRole::Owner.as_str();

    let row = db
        .client
        .query_opt(
            "SELECT m.user_id, n.name, u.email, m.role, m.added_by, m.created_at, m.updated_at
         FROM org_members m
         JOIN users u ON u.id = m.user_id
         JOIN namespaces n ON n.id = u.namespace_id
         WHERE m.org_id = $1 AND m.role = $2",
            &[&org_id.0, &owner_role],
        )
        .await?;

    match row {
        Some(row) => Ok(org_member_from_row(&row)),
        None => Err(Error::OrgMissing(org_id.to_string())),
    }
}

/// Creates an invite to join an organization.
pub async fn create_org_invite(
    db: &AnkhDb,
    org_id: OrgId,
    email: &str,
    invited_by: UserId,
    ttl: Duration,
) -> Result<(String, OrgInviteId)> {
    let token = Uuid::new_v4().to_string();
    let invite_id =
        create_org_invite_with_token(db, org_id, email, invited_by, ttl, &token).await?;
    Ok((token, invite_id))
}

/// Creates an invite with a specific token (for deterministic test seeding).
pub async fn create_org_invite_with_token(
    db: &AnkhDb,
    org_id: OrgId,
    email: &str,
    invited_by: UserId,
    ttl: Duration,
    token: &str,
) -> Result<OrgInviteId> {
    let email_lower = email.to_lowercase();

    // Check for existing pending invite
    let existing = db
        .client
        .query_opt(
            "SELECT id FROM org_invites
         WHERE org_id = $1 AND email = $2
           AND accepted_at IS NULL AND revoked_at IS NULL
           AND expires_at > now()",
            &[&org_id.0, &email_lower],
        )
        .await?;

    if existing.is_some() {
        return Err(Error::OrgInviteAlreadyPending(email_lower));
    }

    // Check if user is already a member
    let already_member = db
        .client
        .query_opt(
            "SELECT 1 FROM org_members m
         JOIN users u ON u.id = m.user_id
         WHERE m.org_id = $1 AND u.email = $2",
            &[&org_id.0, &email_lower],
        )
        .await?;

    if already_member.is_some() {
        // Find the user_id to return proper error
        let user_row = db
            .client
            .query_one("SELECT id FROM users WHERE email = $1", &[&email_lower])
            .await?;
        let user_id: Uuid = user_row.get(0);
        return Err(Error::AlreadyOrgMember(
            user_id.to_string(),
            org_id.to_string(),
        ));
    }

    let token_hash = hash_secret(token);
    let ttl_seconds: i64 = ttl.as_secs().try_into().unwrap_or(i64::MAX);

    let row = db
        .client
        .query_one(
            "INSERT INTO org_invites (token_hash, org_id, email, invited_by, expires_at)
         VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP + ($5::BIGINT * INTERVAL '1 second'))
         RETURNING id",
            &[
                &token_hash,
                &org_id.0,
                &email_lower,
                &invited_by.0,
                &ttl_seconds,
            ],
        )
        .await?;

    let invite_id = OrgInviteId(row.get(0));

    Ok(invite_id)
}

/// Gets an org invite by token (validates but doesn't consume).
pub async fn get_org_invite(db: &AnkhDb, token: &str) -> Result<OrgInvite> {
    let token_hash = hash_secret(token);

    let row = db
        .client
        .query_opt(
            "SELECT id, org_id, email, invited_by, created_at, expires_at,
                accepted_at, revoked_at, accepted_by
         FROM org_invites
         WHERE token_hash = $1",
            &[&token_hash],
        )
        .await?;

    match row {
        Some(row) => {
            let invite = org_invite_from_row(&row);

            // Check if expired
            if invite.expires_at < Utc::now() {
                return Err(Error::OrgInviteExpired(token.to_owned()));
            }

            // Check if already accepted
            if invite.accepted_at.is_some() {
                return Err(Error::OrgInviteAlreadyAccepted);
            }

            // Check if revoked
            if invite.revoked_at.is_some() {
                return Err(Error::OrgInviteRevoked);
            }

            Ok(invite)
        }
        None => Err(Error::OrgInviteNotFound(token.to_owned())),
    }
}

/// Accepts an org invite, adding the user to the organization.
pub async fn accept_org_invite(db: &AnkhDb, token: &str, user_id: UserId) -> Result<()> {
    let invite = get_org_invite(db, token).await?;

    // Get the user's email
    let user_row = db
        .client
        .query_opt("SELECT email FROM users WHERE id = $1", &[&user_id.0])
        .await?;

    let user_email: String = match user_row {
        Some(row) => row.get(0),
        None => return Err(Error::UserMissing(user_id.to_string())),
    };

    // Check email matches
    if user_email.to_lowercase() != invite.email.to_lowercase() {
        return Err(Error::EmailMismatch {
            expected: invite.email,
            actual: user_email,
        });
    }

    // Add user as member
    let member_role = OrgRole::Member.as_str();
    let inserted = db
        .client
        .execute(
            "INSERT INTO org_members (org_id, user_id, role, added_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (org_id, user_id) DO NOTHING",
            &[
                &invite.org_id.0,
                &user_id.0,
                &member_role,
                &invite.invited_by.0,
            ],
        )
        .await?;

    if inserted == 0 {
        return Err(Error::AlreadyOrgMember(
            user_id.to_string(),
            invite.org_id.to_string(),
        ));
    }

    // Mark invite as accepted
    let token_hash = hash_secret(token);
    db.client
        .execute(
            "UPDATE org_invites SET accepted_at = now(), accepted_by = $1
         WHERE token_hash = $2",
            &[&user_id.0, &token_hash],
        )
        .await?;

    Ok(())
}

/// Cancels (revokes) an org invite.
pub async fn cancel_org_invite(db: &AnkhDb, invite_id: OrgInviteId) -> Result<()> {
    let updated = db
        .client
        .execute(
            "UPDATE org_invites SET revoked_at = now()
         WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
            &[&invite_id.0],
        )
        .await?;

    if updated == 0 {
        return Err(Error::OrgInviteNotFound(invite_id.to_string()));
    }

    Ok(())
}

/// Lists all pending invites for an organization.
pub async fn list_org_invites(db: &AnkhDb, org_id: OrgId) -> Result<Vec<OrgInvite>> {
    let rows = db
        .client
        .query(
            "SELECT id, org_id, email, invited_by, created_at, expires_at,
                accepted_at, revoked_at, accepted_by
         FROM org_invites
         WHERE org_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > now()
         ORDER BY created_at DESC",
            &[&org_id.0],
        )
        .await?;

    Ok(rows.iter().map(org_invite_from_row).collect())
}

/// Deletes expired org invites.
pub async fn delete_expired_org_invites(db: &AnkhDb) -> Result<u64> {
    let deleted = db
        .client
        .execute("DELETE FROM org_invites WHERE expires_at < now()", &[])
        .await?;

    Ok(deleted)
}

// Helper functions

/// Converts a database row to an `OrgDetail`.
fn org_detail_from_row(row: &Row) -> OrgDetail {
    OrgDetail {
        id: OrgId(row.get(0)),
        namespace_id: NamespaceId(row.get(1)),
        name: row.get(2),
        display_name: row.get(3),
        created_by: row.get::<_, Option<Uuid>>(4).map(UserId),
        namespace_status: row.get(5),
        namespace_gen: row.get(6),
        created_at: row.get(7),
        updated_at: row.get(8),
    }
}

/// Shared organization creation path with optional reserved-name enforcement.
async fn create_org_inner(
    db: &mut AnkhDb,
    name: &str,
    display_name: Option<&str>,
    created_by: UserId,
    enforce_reserved_check: bool,
) -> Result<OrgId> {
    let validation = if enforce_reserved_check {
        db.validate_namespace_name(name)
    } else {
        db.validate_name_format(name)
    };
    if let Err(message) = validation {
        return Err(Error::InvalidNamespaceName(message));
    }

    let normalized = normalize_name(name);
    let kind = NamespaceKind::Org.as_str();
    let tx = db.client.transaction().await?;
    let namespace_row = tx
        .query_opt(
            "INSERT INTO namespaces (name, kind) VALUES ($1, $2)
             ON CONFLICT (name) DO NOTHING
             RETURNING id",
            &[&normalized, &kind],
        )
        .await?;

    let namespace_id: Uuid = match namespace_row {
        Some(row) => row.get(0),
        None => return Err(Error::NamespaceExists(normalized)),
    };

    let org_row = tx
        .query_one(
            "INSERT INTO organizations (namespace_id, display_name, created_by)
             VALUES ($1, $2, $3)
             RETURNING id",
            &[&namespace_id, &display_name, &created_by.0],
        )
        .await?;
    let org_id = OrgId(org_row.get(0));

    let owner_role = OrgRole::Owner.as_str();
    tx.execute(
        "INSERT INTO org_members (org_id, user_id, role, added_by)
         VALUES ($1, $2, $3, $4)",
        &[&org_id.0, &created_by.0, &owner_role, &created_by.0],
    )
    .await?;

    tx.commit().await?;
    Ok(org_id)
}

/// Converts a database row to an `OrgSummary`.
fn org_summary_from_row(row: &Row) -> OrgSummary {
    OrgSummary {
        id: OrgId(row.get(0)),
        namespace_id: NamespaceId(row.get(1)),
        name: row.get(2),
        display_name: row.get(3),
        created_at: row.get(4),
    }
}

/// Converts a database row to an `OrgMember`.
fn org_member_from_row(row: &Row) -> OrgMember {
    let role_str: String = row.get(3);
    OrgMember {
        user_id: UserId(row.get(0)),
        username: row.get(1),
        email: row.get(2),
        role: OrgRole::parse_db(&role_str).unwrap_or(OrgRole::Member),
        added_by: row.get::<_, Option<Uuid>>(4).map(UserId),
        created_at: row.get(5),
        updated_at: row.get(6),
    }
}

/// Converts a database row to an `OrgInvite`.
fn org_invite_from_row(row: &Row) -> OrgInvite {
    OrgInvite {
        id: OrgInviteId(row.get(0)),
        org_id: OrgId(row.get(1)),
        email: row.get(2),
        invited_by: UserId(row.get(3)),
        created_at: row.get(4),
        expires_at: row.get(5),
        accepted_at: row.get(6),
        revoked_at: row.get(7),
        accepted_by: row.get::<_, Option<Uuid>>(8).map(UserId),
    }
}
