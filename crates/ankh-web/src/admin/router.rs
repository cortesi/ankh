//! Shared admin API router.

use axum::{
    Router,
    routing::{delete, get, post},
};

use super::{auth, device_sessions, namespaces, orgs, sessions, settings, sysadmins, users};

/// Build the mountable Ankh admin router under `/admin/v1`.
pub fn admin_router() -> Router {
    Router::new()
        .route("/admin/v1/auth/login", post(auth::login))
        .route("/admin/v1/sysadmins", get(sysadmins::list_sysadmins))
        .route("/admin/v1/sysadmins/me", get(sysadmins::whoami))
        .route("/admin/v1/users", get(users::list_users))
        .route("/admin/v1/users/release", post(users::release_user))
        .route("/admin/v1/users/invite", post(users::invite_user))
        .route(
            "/admin/v1/users/{id}",
            get(users::get_user).delete(users::delete_user),
        )
        .route("/admin/v1/sessions", get(sessions::list_sessions))
        .route(
            "/admin/v1/sessions/{id}/revoke",
            post(sessions::revoke_session),
        )
        .route(
            "/admin/v1/device-sessions",
            get(device_sessions::list_device_sessions),
        )
        .route(
            "/admin/v1/device-sessions/{id}/revoke",
            post(device_sessions::revoke_device_session),
        )
        .route("/admin/v1/settings", get(settings::get_settings))
        .route("/admin/v1/settings/waitlist", post(settings::set_waitlist))
        .route(
            "/admin/v1/orgs",
            get(orgs::list_orgs).post(orgs::create_org),
        )
        .route(
            "/admin/v1/orgs/{id}",
            get(orgs::get_org)
                .patch(orgs::update_org)
                .delete(orgs::delete_org),
        )
        .route(
            "/admin/v1/orgs/{id}/members",
            get(orgs::list_members).post(orgs::add_member),
        )
        .route(
            "/admin/v1/orgs/{id}/members/{user_id}",
            delete(orgs::remove_member).patch(orgs::set_member_role),
        )
        .route(
            "/admin/v1/orgs/{id}/transfer",
            post(orgs::transfer_ownership),
        )
        .route(
            "/admin/v1/orgs/{id}/invites",
            get(orgs::list_invites).post(orgs::create_invite),
        )
        .route(
            "/admin/v1/orgs/{id}/invites/{invite_id}",
            delete(orgs::cancel_invite),
        )
        .route(
            "/admin/v1/namespaces/{id}/suspend",
            post(namespaces::suspend_namespace),
        )
        .route(
            "/admin/v1/namespaces/{id}/reinstate",
            post(namespaces::reinstate_namespace),
        )
}
