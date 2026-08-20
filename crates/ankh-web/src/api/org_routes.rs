//! Organization handlers for `/api/v1`.

use axum::{Extension, Json, extract::Path};
use serde::Deserialize;

use crate::{
    AnkhWebState, RequireActiveUser, RequireSession,
    api::ApiResult,
    services::orgs::{self, CreateOrgInput},
};

/// Organization invite creation payload.
#[derive(Deserialize)]
pub struct InviteRequest {
    /// Email address to invite into the organization.
    pub invite_email: String,
}

/// List organizations visible to the current user.
pub async fn list_orgs(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
) -> ApiResult<Json<Vec<orgs::OrgInfo>>> {
    Ok(Json(orgs::list_my_orgs(&state, &session).await?))
}

/// Create a new organization owned by the current user.
pub async fn create_org(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Json(input): Json<CreateOrgInput>,
) -> ApiResult<Json<orgs::OrgInfo>> {
    Ok(Json(orgs::create_org(&state, &session, input).await?))
}

/// Fetch an organization by ID.
pub async fn get_org(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path(id): Path<String>,
) -> ApiResult<Json<orgs::OrgInfo>> {
    Ok(Json(orgs::get_org(&state, &session, id).await?))
}

/// Fetch the current user's membership for an organization.
pub async fn get_my_membership(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path(id): Path<String>,
) -> ApiResult<Json<orgs::OrgMemberInfo>> {
    Ok(Json(orgs::get_my_membership(&state, &session, id).await?))
}

/// Leave an organization as the current user.
pub async fn leave_org(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path(id): Path<String>,
) -> ApiResult<Json<()>> {
    orgs::leave_org(&state, &session, id).await?;
    Ok(Json(()))
}

/// List members for an organization.
pub async fn list_members(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<orgs::MemberInfo>>> {
    Ok(Json(orgs::list_org_members(&state, &session, id).await?))
}

/// Remove an organization member by membership ID.
pub async fn remove_member(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path((org_id, member_id)): Path<(String, String)>,
) -> ApiResult<Json<()>> {
    orgs::remove_org_member(&state, &session, org_id, member_id).await?;
    Ok(Json(()))
}

/// List pending invites for an organization.
pub async fn list_invites(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<orgs::InviteInfo>>> {
    Ok(Json(orgs::list_org_invites(&state, &session, id).await?))
}

/// Create an organization invite and send its email.
pub async fn invite_to_org(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path(id): Path<String>,
    Json(request): Json<InviteRequest>,
) -> ApiResult<Json<orgs::InviteInfo>> {
    Ok(Json(
        orgs::invite_to_org(&state, &session, id, request.invite_email).await?,
    ))
}

/// Cancel a pending organization invite.
pub async fn cancel_invite(
    Extension(state): Extension<AnkhWebState>,
    RequireActiveUser(session): RequireActiveUser,
    Path((org_id, invite_id)): Path<(String, String)>,
) -> ApiResult<Json<()>> {
    orgs::cancel_org_invite(&state, &session, org_id, invite_id).await?;
    Ok(Json(()))
}

/// Resolve invite metadata from a public token.
pub async fn get_invite_details(
    Extension(state): Extension<AnkhWebState>,
    Path(token): Path<String>,
) -> ApiResult<Json<orgs::OrgInviteDetails>> {
    Ok(Json(orgs::get_org_invite_details(&state, token).await?))
}

/// Accept an organization invite as the current user.
///
/// Uses [`RequireSession`] (not [`RequireActiveUser`]): accepting an org invite is
/// an onboarding path that a waitlisted user must be able to complete.
pub async fn accept_invite(
    Extension(state): Extension<AnkhWebState>,
    RequireSession(session): RequireSession,
    Path(token): Path<String>,
) -> ApiResult<Json<orgs::OrgInfo>> {
    Ok(Json(
        orgs::accept_org_invite(&state, &session, token).await?,
    ))
}
