//! Shared admin organization management handlers.

use std::collections::HashMap;

use ankh_constants::ORG_INVITE_TTL;
use ankh_db::{Error as DbError, OrgRole};
use ankh_mail::template;
use ankh_types::admin::{
    AddMemberRequest, CreateOrgInviteRequest, CreateOrgInviteResponse, CreateOrgRequest,
    ListMembersResponse, ListOrgInvitesResponse, ListOrgsResponse, SetRoleRequest,
    TransferOwnershipRequest, UpdateOrgRequest,
};
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use super::{
    audit::{AdminAuditEvent, AdminAuditResult, RequestContext, emit_admin_audit},
    conversions::{org_detail, org_invite, org_member, org_summary},
    error::{AdminError, AdminResult},
    ids::{parse_org_id, parse_org_invite_id, parse_user_id},
    middleware::SysadminAuth,
    pagination::{clamp_limit, default_limit},
};
use crate::{AnkhWebState, NamespaceDeleted, OrgMemberRemoved};

/// Query parameters for listing orgs.
#[derive(Debug, Deserialize)]
pub struct ListOrgsQuery {
    /// Maximum number of orgs to return.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Pagination cursor for fetching subsequent pages.
    pub cursor: Option<String>,
}

/// Helper to parse org role from string.
fn parse_role(role: &str) -> Result<OrgRole, AdminError> {
    match role.to_lowercase().as_str() {
        "owner" => Ok(OrgRole::Owner),
        "admin" => Ok(OrgRole::Admin),
        "member" => Ok(OrgRole::Member),
        _ => Err(AdminError::bad_request("invalid role")),
    }
}

/// List all organizations with optional pagination.
pub async fn list_orgs(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Query(query): Query<ListOrgsQuery>,
) -> AdminResult<impl IntoResponse> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let (orgs, next_cursor) = db
        .list_all_orgs(clamp_limit(query.limit), query.cursor.as_deref())
        .await
        .map_err(|error| AdminError::internal(format!("list orgs error: {error}")))?;

    Ok((
        StatusCode::OK,
        Json(ListOrgsResponse {
            orgs: orgs.into_iter().map(org_summary).collect(),
            next_cursor,
        }),
    ))
}

/// Get detailed information about an organization.
pub async fn get_org(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let org = db
        .get_org_by_id(org_id)
        .await
        .map_err(|error| match error {
            DbError::OrgMissing(_) => AdminError::not_found("org not found"),
            _ => AdminError::internal(format!("get org error: {error}")),
        })?;

    Ok((StatusCode::OK, Json(org_detail(org))))
}

/// Create a new organization.
pub async fn create_org(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Json(request): Json<CreateOrgRequest>,
) -> AdminResult<impl IntoResponse> {
    let owner_id = parse_user_id(request.owner_id.as_str())?;
    let result = async {
        let mut db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        let org_id = db
            .create_org(
                request.name.as_str(),
                request.display_name.as_deref(),
                owner_id,
            )
            .await
            .map_err(|error| match error {
                DbError::NamespaceExists(_) => AdminError::conflict("org name already exists"),
                DbError::InvalidNamespaceName(message) => AdminError::bad_request(message),
                DbError::UserMissing(_) => AdminError::not_found("owner not found"),
                _ => AdminError::internal(format!("create org error: {error}")),
            })?;
        db.get_org_by_id(org_id)
            .await
            .map_err(|error| AdminError::internal(format!("get org error: {error}")))
    }
    .await;

    audit_mutation(
        &state,
        admin.id,
        "org.create",
        "org",
        request.name,
        &result,
        &ctx,
    )
    .await;
    let org = result?;
    Ok((StatusCode::CREATED, Json(org_detail(org))))
}

/// Update an organization.
pub async fn update_org(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateOrgRequest>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.update_org(org_id, request.display_name.as_deref())
            .await
            .map_err(|error| match error {
                DbError::OrgMissing(_) => AdminError::not_found("org not found"),
                _ => AdminError::internal(format!("update org error: {error}")),
            })?;
        db.get_org_by_id(org_id)
            .await
            .map_err(|error| AdminError::internal(format!("get org error: {error}")))
    }
    .await;

    audit_mutation(&state, admin.id, "org.update", "org", id, &result, &ctx).await;
    let org = result?;
    Ok((StatusCode::OK, Json(org_detail(org))))
}

/// Delete an organization.
pub async fn delete_org(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        let org = db
            .get_org_by_id(org_id)
            .await
            .map_err(|error| match error {
                DbError::OrgMissing(_) => AdminError::not_found("org not found"),
                _ => AdminError::internal(format!("get org error: {error}")),
            })?;
        db.delete_org(org_id).await.map_err(|error| match error {
            DbError::OrgMissing(_) => AdminError::not_found("org not found"),
            DbError::OrgNotEmpty(_) => {
                AdminError::conflict("org has members other than owner; remove them first")
            }
            _ => AdminError::internal(format!("delete org error: {error}")),
        })?;
        Ok(NamespaceDeleted {
            namespace_id: org.namespace_id,
            namespace: org.name,
        })
    }
    .await;

    audit_mutation(&state, admin.id, "org.delete", "org", id, &result, &ctx).await;
    let deleted = result?;
    if let Err(error) = state.hooks().on_namespaces_deleted(vec![deleted]).await {
        state.record_hook_failure("on_namespaces_deleted", error);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// List organization members.
pub async fn list_members(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let members = db
        .list_org_members(org_id)
        .await
        .map_err(|error| match error {
            DbError::OrgMissing(_) => AdminError::not_found("org not found"),
            _ => AdminError::internal(format!("list members error: {error}")),
        })?;

    Ok((
        StatusCode::OK,
        Json(ListMembersResponse {
            members: members.into_iter().map(org_member).collect(),
        }),
    ))
}

/// Add a member to an organization.
pub async fn add_member(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
    Json(request): Json<AddMemberRequest>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let user_id = parse_user_id(request.user_id.as_str())?;
    let role = parse_role(request.role.as_str())?;
    if role == OrgRole::Owner {
        return Err(AdminError::bad_request(
            "cannot add owner directly; use transfer",
        ));
    }

    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.add_org_member(org_id, user_id, role, None)
            .await
            .map_err(|error| match error {
                DbError::OrgMissing(_) => AdminError::not_found("org not found"),
                DbError::UserMissing(_) => AdminError::not_found("user not found"),
                DbError::AlreadyOrgMember(_, _) => AdminError::conflict("user is already a member"),
                _ => AdminError::internal(format!("add member error: {error}")),
            })
    }
    .await;

    let target = format!("{}:{}", id, request.user_id);
    audit_mutation(
        &state,
        admin.id,
        "org.member.add",
        "org_member",
        target,
        &result,
        &ctx,
    )
    .await;
    result?;
    Ok(StatusCode::CREATED)
}

/// Remove a member from an organization.
pub async fn remove_member(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path((id, user_id)): Path<(String, String)>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let user_id_value = parse_user_id(user_id.as_str())?;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        let org = db
            .get_org_by_id(org_id)
            .await
            .map_err(|error| match error {
                DbError::OrgMissing(_) => AdminError::not_found("org not found"),
                _ => AdminError::internal(format!("get org error: {error}")),
            })?;
        let target =
            db.get_org_member(org_id, user_id_value)
                .await
                .map_err(|error| match error {
                    DbError::NotOrgMember(_, _) => AdminError::not_found("user is not a member"),
                    _ => AdminError::internal(format!("get member error: {error}")),
                })?;
        db.remove_org_member(org_id, user_id_value)
            .await
            .map_err(|error| match error {
                DbError::OrgMissing(_) => AdminError::not_found("org not found"),
                DbError::NotOrgMember(_, _) => AdminError::not_found("user is not a member"),
                DbError::PermissionDenied(_) => AdminError::forbidden("cannot remove owner"),
                _ => AdminError::internal(format!("remove member error: {error}")),
            })?;
        Ok(OrgMemberRemoved {
            namespace: org.name,
            user_id: target.user_id,
        })
    }
    .await;

    let target = format!("{id}:{user_id}");
    audit_mutation(
        &state,
        admin.id,
        "org.member.remove",
        "org_member",
        target,
        &result,
        &ctx,
    )
    .await;
    let removed = result?;
    if let Err(error) = state.hooks().on_org_member_removed(removed).await {
        state.record_hook_failure("on_org_member_removed", error);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Set a member's role.
pub async fn set_member_role(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path((id, user_id)): Path<(String, String)>,
    Json(request): Json<SetRoleRequest>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let user_id_value = parse_user_id(user_id.as_str())?;
    let role = parse_role(request.role.as_str())?;
    if role == OrgRole::Owner {
        return Err(AdminError::bad_request(
            "cannot set owner role directly; use transfer",
        ));
    }

    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.set_org_member_role(org_id, user_id_value, role)
            .await
            .map_err(|error| match error {
                DbError::OrgMissing(_) => AdminError::not_found("org not found"),
                DbError::NotOrgMember(_, _) => AdminError::not_found("user is not a member"),
                DbError::PermissionDenied(_) => AdminError::forbidden("cannot change owner role"),
                _ => AdminError::internal(format!("set role error: {error}")),
            })
    }
    .await;

    let target = format!("{id}:{user_id}");
    audit_mutation(
        &state,
        admin.id,
        "org.member.role",
        "org_member",
        target,
        &result,
        &ctx,
    )
    .await;
    result?;
    Ok(StatusCode::NO_CONTENT)
}

/// Transfer ownership of an organization.
pub async fn transfer_ownership(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
    Json(request): Json<TransferOwnershipRequest>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let new_owner_id = parse_user_id(request.new_owner_id.as_str())?;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.transfer_org_ownership(org_id, new_owner_id)
            .await
            .map_err(|error| match error {
                DbError::OrgMissing(_) => AdminError::not_found("org not found"),
                DbError::NotOrgMember(_, _) => {
                    AdminError::bad_request("new owner must be a member")
                }
                _ => AdminError::internal(format!("transfer error: {error}")),
            })
    }
    .await;

    let target = format!("{}:{}", id, request.new_owner_id);
    audit_mutation(
        &state,
        admin.id,
        "org.transfer",
        "org",
        target,
        &result,
        &ctx,
    )
    .await;
    result?;
    Ok(StatusCode::NO_CONTENT)
}

/// List organization invites.
pub async fn list_invites(
    _auth: SysadminAuth,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let invites = db
        .list_org_invites(org_id)
        .await
        .map_err(|error| match error {
            DbError::OrgMissing(_) => AdminError::not_found("org not found"),
            _ => AdminError::internal(format!("list invites error: {error}")),
        })?;

    Ok((
        StatusCode::OK,
        Json(ListOrgInvitesResponse {
            invites: invites.into_iter().map(org_invite).collect(),
        }),
    ))
}

/// Create an organization invite.
pub async fn create_invite(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path(id): Path<String>,
    Json(request): Json<CreateOrgInviteRequest>,
) -> AdminResult<impl IntoResponse> {
    let org_id = parse_org_id(id.as_str())?;
    let result = create_invite_inner(&state, org_id, request.email).await;
    audit_mutation(
        &state,
        admin.id,
        "org.invite.create",
        "org_invite",
        &id,
        &result,
        &ctx,
    )
    .await;

    let (id, email, token) = result?;
    Ok((
        StatusCode::CREATED,
        Json(CreateOrgInviteResponse { id, email, token }),
    ))
}

/// Create an invite and send mail.
async fn create_invite_inner(
    state: &AnkhWebState,
    org_id: ankh_db::OrgId,
    email: String,
) -> AdminResult<(String, String, String)> {
    let db = state
        .db_pool()
        .get()
        .await
        .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
    let org = db
        .get_org_by_id(org_id)
        .await
        .map_err(|error| match error {
            DbError::OrgMissing(_) => AdminError::not_found("org not found"),
            _ => AdminError::internal(format!("get org error: {error}")),
        })?;
    let owner = db
        .get_org_owner(org_id)
        .await
        .map_err(|error| AdminError::internal(format!("get owner error: {error}")))?;
    let (token, invite_id) = db
        .create_org_invite(org_id, email.as_str(), owner.user_id, ORG_INVITE_TTL)
        .await
        .map_err(|error| match error {
            DbError::OrgMissing(_) => AdminError::not_found("org not found"),
            DbError::OrgInviteAlreadyPending(_) => {
                AdminError::conflict("pending invite already exists for this email")
            }
            _ => AdminError::internal(format!("create invite error: {error}")),
        })?;

    let org_name = org.display_name.unwrap_or(org.name);
    let invite_url = format!("{}?org_invite={token}", state.mail().link_url("/signup"));
    let vars = HashMap::from([
        ("org_name".to_owned(), org_name),
        ("invite_url".to_owned(), invite_url),
    ]);
    let email_to_send = state
        .mail()
        .render_email(template::ORG_INVITE, email.as_str(), &vars)
        .map_err(|error| AdminError::internal(format!("mail render error: {error}")))?;
    state
        .mail()
        .send(&email_to_send)
        .await
        .map_err(|error| AdminError::internal(format!("mail send error: {error}")))?;

    Ok((invite_id.to_string(), email, token))
}

/// Cancel an organization invite.
pub async fn cancel_invite(
    SysadminAuth(admin): SysadminAuth,
    ctx: RequestContext,
    Extension(state): Extension<AnkhWebState>,
    Path((id, invite_id)): Path<(String, String)>,
) -> AdminResult<impl IntoResponse> {
    let invite_id_value = parse_org_invite_id(invite_id.as_str())?;
    let result = async {
        let db = state
            .db_pool()
            .get()
            .await
            .map_err(|error| AdminError::internal(format!("database error: {error}")))?;
        db.cancel_org_invite(invite_id_value)
            .await
            .map_err(|error| match error {
                DbError::OrgInviteNotFound(_) => AdminError::not_found("invite not found"),
                _ => AdminError::internal(format!("cancel invite error: {error}")),
            })
    }
    .await;

    let target = format!("{id}:{invite_id}");
    audit_mutation(
        &state,
        admin.id,
        "org.invite.cancel",
        "org_invite",
        target,
        &result,
        &ctx,
    )
    .await;
    result?;
    Ok(StatusCode::NO_CONTENT)
}

/// Emit mutation audit with success/failure status.
async fn audit_mutation<T>(
    state: &AnkhWebState,
    admin_id: ankh_db::SysadminId,
    action: &'static str,
    target_type: &'static str,
    target_id: impl Into<String>,
    result: &AdminResult<T>,
    ctx: &RequestContext,
) {
    let audit_result = AdminAuditResult::from(result.is_ok());
    emit_admin_audit(
        state,
        AdminAuditEvent::new(
            Some(admin_id),
            action,
            target_type,
            target_id,
            audit_result,
            ctx,
        ),
    )
    .await;
}
