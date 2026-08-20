//! Organization services.

use std::collections::HashMap;

use ankh_constants::ORG_INVITE_TTL;
use ankh_db::{Error as DbError, OrgRole, Session};
use ankh_mail::template;
pub use ankh_types::{
    CreateOrgInput, InviteInfo, MemberInfo, OrgInfo, OrgInviteDetails, OrgMemberInfo,
};
use uuid::Uuid;

use crate::{
    api::{ApiError, ApiResult},
    auth::{bad_request, unauthorized},
    errors,
    hooks::OrgMemberRemoved,
    state::AnkhWebState,
};

/// List organizations the current user belongs to.
pub async fn list_my_orgs(state: &AnkhWebState, session: &Session) -> ApiResult<Vec<OrgInfo>> {
    let email = session.email.clone();
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let user = db
        .get_user_by_email(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let mut result = vec![OrgInfo {
        id: user.namespace_id.to_string(),
        name: user.username.clone(),
        display_name: Some(user.username.clone()),
        role: OrgRole::Owner.as_str().to_owned(),
        is_personal: true,
    }];

    let orgs = db
        .list_orgs_for_user(user.id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    for org in orgs {
        let member = db
            .get_org_member(org.id, user.id)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        result.push(OrgInfo {
            id: org.id.to_string(),
            name: org.name,
            display_name: org.display_name,
            role: member.role.as_str().to_owned(),
            is_personal: false,
        });
    }
    Ok(result)
}

/// Create a new organization with the current user as owner.
pub async fn create_org(
    state: &AnkhWebState,
    session: &Session,
    input: CreateOrgInput,
) -> ApiResult<OrgInfo> {
    let email = session.email.clone();
    let mut db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let user = db
        .get_user_by_email(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let org_id = db
        .create_org(input.name.as_str(), input.display_name.as_deref(), user.id)
        .await
        .map_err(|err| match err {
            DbError::NamespaceExists(_) => bad_request(errors::ORG_NAME_TAKEN),
            DbError::InvalidNamespaceName(msg) => bad_request(msg),
            _ => ApiError::internal(err.to_string()),
        })?;

    let org = db
        .get_org_by_id(org_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(OrgInfo {
        id: org.id.to_string(),
        name: org.name,
        display_name: org.display_name,
        role: OrgRole::Owner.as_str().to_owned(),
        is_personal: false,
    })
}

/// Get organization details if the current user is a member.
pub async fn get_org(
    state: &AnkhWebState,
    session: &Session,
    org_id: String,
) -> ApiResult<OrgInfo> {
    let email = session.email.clone();
    let id: Uuid = org_id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_ORG_ID))?;

    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let user = db
        .get_user_by_email(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    if user.namespace_id.0 == id {
        return Ok(OrgInfo {
            id: user.namespace_id.to_string(),
            name: user.username.clone(),
            display_name: Some(user.username),
            role: OrgRole::Owner.as_str().to_owned(),
            is_personal: true,
        });
    }

    let org_id = ankh_types::OrgId(id);
    let member = db
        .get_org_member(org_id, user.id)
        .await
        .map_err(|err| match err {
            DbError::OrgMissing(_) => bad_request(errors::ORG_NOT_FOUND),
            DbError::NotOrgMember(_, _) => unauthorized(errors::NOT_ORG_MEMBER),
            _ => ApiError::internal(err.to_string()),
        })?;
    let org = db
        .get_org_by_id(org_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(OrgInfo {
        id: org.id.to_string(),
        name: org.name,
        display_name: org.display_name,
        role: member.role.as_str().to_owned(),
        is_personal: false,
    })
}

/// Get the current user's membership in an organization.
pub async fn get_my_membership(
    state: &AnkhWebState,
    session: &Session,
    org_id: String,
) -> ApiResult<OrgMemberInfo> {
    let email = session.email.clone();
    let org_id: ankh_types::OrgId = org_id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_ORG_ID))?;

    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let user = db
        .get_user_by_email(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let member = db
        .get_org_member(org_id, user.id)
        .await
        .map_err(|err| match err {
            DbError::OrgMissing(_) => bad_request(errors::ORG_NOT_FOUND),
            DbError::NotOrgMember(_, _) => unauthorized(errors::NOT_ORG_MEMBER),
            _ => ApiError::internal(err.to_string()),
        })?;

    Ok(OrgMemberInfo {
        user_id: user.id.to_string(),
        username: user.username,
        role: member.role.as_str().to_owned(),
    })
}

/// Leave an organization.
pub async fn leave_org(state: &AnkhWebState, session: &Session, org_id: String) -> ApiResult<()> {
    let email = session.email.clone();
    let org_id: ankh_types::OrgId = org_id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_ORG_ID))?;

    let removed = {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let user = db
            .get_user_by_email(email.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        remove_member_unchecked(&db, org_id, user.id, user.id, false, true).await?
    };
    dispatch_org_member_removed(state, removed).await;
    Ok(())
}

/// List members of an organization.
pub async fn list_org_members(
    state: &AnkhWebState,
    session: &Session,
    org_id: String,
) -> ApiResult<Vec<MemberInfo>> {
    let email = session.email.clone();
    let id: Uuid = org_id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_ORG_ID))?;

    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let user = db
        .get_user_by_email(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    if user.namespace_id.0 == id {
        return Err(bad_request(errors::PERSONAL_ORG_NO_MEMBERS));
    }

    let org_id = ankh_types::OrgId(id);
    db.get_org_member(org_id, user.id)
        .await
        .map_err(|err| match err {
            DbError::OrgMissing(_) => bad_request(errors::ORG_NOT_FOUND),
            DbError::NotOrgMember(_, _) => unauthorized(errors::NOT_ORG_MEMBER),
            _ => ApiError::internal(err.to_string()),
        })?;

    let members = db
        .list_org_members(org_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    Ok(members
        .into_iter()
        .map(|member| MemberInfo {
            user_id: member.user_id.to_string(),
            username: member.username,
            email: member.email,
            role: member.role.as_str().to_owned(),
        })
        .collect())
}

/// List pending invites for an organization.
pub async fn list_org_invites(
    state: &AnkhWebState,
    session: &Session,
    org_id: String,
) -> ApiResult<Vec<InviteInfo>> {
    let email = session.email.clone();
    let (db, org_id) = load_org_for_privileged_member(state, email, org_id).await?;
    let invites = db
        .list_org_invites(org_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    Ok(invites
        .into_iter()
        .map(|invite| InviteInfo {
            id: invite.id.to_string(),
            email: invite.email,
            created_at: invite.created_at.to_rfc3339(),
            expires_at: invite.expires_at.to_rfc3339(),
        })
        .collect())
}

/// Invite a user to an organization.
pub async fn invite_to_org(
    state: &AnkhWebState,
    session: &Session,
    org_id: String,
    invite_email: String,
) -> ApiResult<InviteInfo> {
    let email = session.email.clone();
    let invite_email = invite_email.trim().to_lowercase();
    let (db, org_id) = load_org_for_privileged_member(state, email, org_id).await?;
    let user = db
        .get_user_by_email(session.email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let org = db
        .get_org_by_id(org_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let org_name = org.display_name.unwrap_or(org.name);

    let (token, invite_id) = db
        .create_org_invite(org_id, invite_email.as_str(), user.id, ORG_INVITE_TTL)
        .await
        .map_err(|err| match err {
            DbError::OrgInviteAlreadyPending(_) => bad_request(errors::INVITE_ALREADY_PENDING),
            DbError::AlreadyOrgMember(_, _) => bad_request(errors::ALREADY_ORG_MEMBER),
            _ => ApiError::internal(err.to_string()),
        })?;

    let invite_url = format!("{}?org_invite={token}", state.mail().link_url("/signup"));
    let vars = HashMap::from([
        ("org_name".to_string(), org_name),
        ("invite_url".to_string(), invite_url),
    ]);
    let email_to_send =
        state
            .mail()
            .render_email(template::ORG_INVITE, invite_email.as_str(), &vars)?;
    state.mail().send(&email_to_send).await?;

    let invite = db
        .list_org_invites(org_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?
        .into_iter()
        .find(|invite| invite.id == invite_id)
        .ok_or_else(|| ApiError::internal("invite not found after creation"))?;

    Ok(InviteInfo {
        id: invite.id.to_string(),
        email: invite.email,
        created_at: invite.created_at.to_rfc3339(),
        expires_at: invite.expires_at.to_rfc3339(),
    })
}

/// Cancel an organization invite.
pub async fn cancel_org_invite(
    state: &AnkhWebState,
    session: &Session,
    org_id: String,
    invite_id: String,
) -> ApiResult<()> {
    let email = session.email.clone();
    let invite_id: ankh_types::OrgInviteId = invite_id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_INVITE_ID))?;
    let (db, _) = load_org_for_privileged_member(state, email, org_id).await?;

    db.cancel_org_invite(invite_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))
}

/// Remove a member from an organization.
pub async fn remove_org_member(
    state: &AnkhWebState,
    session: &Session,
    org_id: String,
    member_id: String,
) -> ApiResult<()> {
    let email = session.email.clone();
    let id: Uuid = org_id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_ORG_ID))?;
    let member_id: ankh_types::UserId = member_id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_USER_ID))?;

    let removed = {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        let requester = db
            .get_user_by_email(email.as_str())
            .await
            .map_err(|err| ApiError::internal(err.to_string()))?;
        if requester.namespace_id.0 == id {
            return Err(bad_request(errors::PERSONAL_ORG_NO_MEMBERS));
        }
        remove_member_unchecked(
            &db,
            ankh_types::OrgId(id),
            requester.id,
            member_id,
            false,
            false,
        )
        .await?
    };
    dispatch_org_member_removed(state, removed).await;
    Ok(())
}

/// Remove a member without parsing route inputs.
pub async fn remove_member_unchecked(
    db: &ankh_db::AnkhDb,
    org_id: ankh_types::OrgId,
    requester_id: ankh_types::UserId,
    member_id: ankh_types::UserId,
    bypass_permission: bool,
    allow_self_leave: bool,
) -> ApiResult<OrgMemberRemoved> {
    let requester = db
        .get_org_member(org_id, requester_id)
        .await
        .map_err(|err| match err {
            DbError::OrgMissing(_) => bad_request(errors::ORG_NOT_FOUND),
            DbError::NotOrgMember(_, _) => unauthorized(errors::NOT_ORG_MEMBER),
            _ => ApiError::internal(err.to_string()),
        })?;
    let target = db
        .get_org_member(org_id, member_id)
        .await
        .map_err(|err| match err {
            DbError::NotOrgMember(_, _) => bad_request(errors::NOT_ORG_MEMBER),
            _ => ApiError::internal(err.to_string()),
        })?;

    if !(bypass_permission || allow_self_leave && requester_id == member_id) {
        match requester.role {
            OrgRole::Owner => {
                if target.user_id == requester_id {
                    return Err(bad_request(errors::OWNER_CANNOT_LEAVE));
                }
            }
            OrgRole::Admin => {
                if target.role != OrgRole::Member {
                    return Err(unauthorized(errors::PERMISSION_DENIED));
                }
            }
            OrgRole::Member => return Err(unauthorized(errors::PERMISSION_DENIED)),
        }
    } else if allow_self_leave && target.role == OrgRole::Owner {
        return Err(bad_request(errors::OWNER_CANNOT_LEAVE));
    }

    let org = db.get_org_by_id(org_id).await.map_err(|err| match err {
        DbError::OrgMissing(_) => bad_request(errors::ORG_NOT_FOUND),
        _ => ApiError::internal(err.to_string()),
    })?;
    db.remove_org_member(org_id, member_id)
        .await
        .map_err(|err| match err {
            DbError::OrgMissing(_) => bad_request(errors::ORG_NOT_FOUND),
            _ => ApiError::internal(err.to_string()),
        })?;
    Ok(OrgMemberRemoved {
        namespace: org.name,
        user_id: target.user_id,
    })
}

/// Get org invite details by token.
pub async fn get_org_invite_details(
    state: &AnkhWebState,
    token: String,
) -> ApiResult<OrgInviteDetails> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let invite = db
        .get_org_invite(token.as_str())
        .await
        .map_err(|err| match err {
            DbError::OrgInviteNotFound(_)
            | DbError::OrgInviteExpired(_)
            | DbError::OrgInviteRevoked
            | DbError::OrgInviteAlreadyAccepted => bad_request(errors::INVALID_ORG_INVITE),
            _ => ApiError::internal(err.to_string()),
        })?;
    let org = db
        .get_org_by_id(invite.org_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(OrgInviteDetails {
        org_name: org.name,
        org_display_name: org.display_name,
        invite_email: invite.email,
    })
}

/// Accept an org invite for the currently logged-in user.
pub async fn accept_org_invite(
    state: &AnkhWebState,
    session: &Session,
    token: String,
) -> ApiResult<OrgInfo> {
    let email = session.email.clone();
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let user = db
        .get_user_by_email(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let invite = db
        .get_org_invite(token.as_str())
        .await
        .map_err(|err| match err {
            DbError::OrgInviteNotFound(_)
            | DbError::OrgInviteExpired(_)
            | DbError::OrgInviteRevoked
            | DbError::OrgInviteAlreadyAccepted => bad_request(errors::INVALID_ORG_INVITE),
            _ => ApiError::internal(err.to_string()),
        })?;

    if invite.email != email {
        return Err(bad_request(errors::INVITE_EMAIL_MISMATCH));
    }

    db.accept_org_invite(token.as_str(), user.id)
        .await
        .map_err(|err| match err {
            DbError::OrgInviteNotFound(_)
            | DbError::OrgInviteExpired(_)
            | DbError::OrgInviteRevoked
            | DbError::OrgInviteAlreadyAccepted => bad_request(errors::INVALID_ORG_INVITE),
            DbError::AlreadyOrgMember(_, _) => bad_request(errors::ALREADY_ORG_MEMBER),
            _ => ApiError::internal(err.to_string()),
        })?;
    let org = db
        .get_org_by_id(invite.org_id)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Ok(OrgInfo {
        id: org.id.to_string(),
        name: org.name,
        display_name: org.display_name,
        role: OrgRole::Member.as_str().to_owned(),
        is_personal: false,
    })
}

/// Load an org only when the session user is an admin or owner.
async fn load_org_for_privileged_member(
    state: &AnkhWebState,
    email: String,
    org_id: String,
) -> ApiResult<(ankh_db::AnkhDb, ankh_types::OrgId)> {
    let id: Uuid = org_id
        .parse()
        .map_err(|_| bad_request(errors::INVALID_ORG_ID))?;
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let user = db
        .get_user_by_email(email.as_str())
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    if user.namespace_id.0 == id {
        return Err(bad_request(errors::PERSONAL_ORG_NO_MEMBERS));
    }

    let org_id = ankh_types::OrgId(id);
    let member = db
        .get_org_member(org_id, user.id)
        .await
        .map_err(|err| match err {
            DbError::OrgMissing(_) => bad_request(errors::ORG_NOT_FOUND),
            DbError::NotOrgMember(_, _) => unauthorized(errors::NOT_ORG_MEMBER),
            _ => ApiError::internal(err.to_string()),
        })?;
    if member.role == OrgRole::Member {
        return Err(unauthorized(errors::PERMISSION_DENIED));
    }
    Ok((db, org_id))
}

/// Dispatch org-member removal hooks best-effort.
async fn dispatch_org_member_removed(state: &AnkhWebState, payload: OrgMemberRemoved) {
    if let Err(error) = state.hooks().on_org_member_removed(payload).await {
        state.record_hook_failure("on_org_member_removed", error);
    }
}
