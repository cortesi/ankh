//! HTTP client for the admin API.

use ankh_types::admin::{
    AddMemberRequest, AdminLoginRequest as LoginRequest, CreateOrgInviteRequest,
    CreateOrgInviteResponse, CreateOrgRequest, InviteUserRequest, InviteUserResponse,
    ListDeviceSessionsResponse, ListMembersResponse, ListOrgInvitesResponse, ListOrgsResponse,
    ListSessionsResponse, ListSysadminsResponse, ListUsersResponse, LoginResponse, OrgDetail,
    ReleaseUserRequest, ReleaseUserResponse, SetRoleRequest, SettingsResponse,
    TransferOwnershipRequest, UpdateOrgRequest, UserDetail, WaitlistSettingsRequest,
    WhoamiResponse,
};
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::from_str as parse_json;

use crate::error::{Error, Result};

/// API error response structure matching the admin API.
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    /// Error details.
    error: ApiErrorDetail,
}

/// API error detail structure.
#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    /// Error code.
    code: String,
    /// Error message.
    message: String,
}

/// Admin API client.
pub struct AdminClient {
    /// HTTP client.
    client: Client,
    /// Base URL for the admin API.
    base_url: String,
    /// Bearer token for authentication.
    token: Option<String>,
    /// Optional trace ID to send with requests.
    trace_id: Option<String>,
}

impl AdminClient {
    /// Create a new admin client.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            token: None,
            trace_id: None,
        }
    }

    /// Set the bearer token for authentication.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Set the trace ID for request correlation.
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    /// Build a request with common headers.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);

        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }

        if let Some(trace_id) = &self.trace_id {
            req = req.header("x-request-id", trace_id);
        }

        req
    }

    /// Execute a request and parse the JSON response.
    pub async fn execute<T: DeserializeOwned>(&self, req: RequestBuilder) -> Result<T> {
        let response = req.send().await?;
        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            let parsed = parse_json::<T>(&body).map_err(|err| {
                Error::InvalidResponse(format!("{} (body: {})", err, truncate_body(&body)))
            })?;
            Ok(parsed)
        } else {
            Err(api_error(status, &body))
        }
    }

    /// Execute a request that returns no content (204).
    pub async fn execute_no_content(&self, req: RequestBuilder) -> Result<()> {
        let response = req.send().await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            let body = response.text().await?;
            Err(api_error(status, &body))
        }
    }

    /// POST /admin/v1/auth/login
    pub async fn login(&self, email: &str, password: &str) -> Result<LoginResponse> {
        let req = self
            .request(Method::POST, "/admin/v1/auth/login")
            .json(&LoginRequest {
                email: email.to_string(),
                password: password.to_string(),
            });
        self.execute(req).await
    }

    /// GET /admin/v1/sysadmins/me
    pub async fn whoami(&self) -> Result<WhoamiResponse> {
        let req = self.request(Method::GET, "/admin/v1/sysadmins/me");
        self.execute(req).await
    }

    /// GET /admin/v1/users
    pub async fn list_users(&self, params: &ListUsersParams) -> Result<ListUsersResponse> {
        let mut req = self.request(Method::GET, "/admin/v1/users");
        if let Some(limit) = params.limit {
            req = req.query(&[("limit", limit.to_string())]);
        }
        if let Some(cursor) = &params.cursor {
            req = req.query(&[("cursor", cursor)]);
        }
        if let Some(email) = &params.email {
            req = req.query(&[("email", email)]);
        }
        self.execute(req).await
    }

    /// GET /admin/v1/users/:id
    pub async fn get_user(&self, id: &str) -> Result<UserDetail> {
        let req = self.request(Method::GET, &format!("/admin/v1/users/{}", id));
        self.execute(req).await
    }

    /// DELETE /admin/v1/users/:id
    pub async fn delete_user(&self, id: &str) -> Result<()> {
        let req = self.request(Method::DELETE, &format!("/admin/v1/users/{}", id));
        self.execute_no_content(req).await
    }

    /// GET /admin/v1/sessions
    pub async fn list_sessions(&self, params: &ListSessionsParams) -> Result<ListSessionsResponse> {
        let mut req = self.request(Method::GET, "/admin/v1/sessions");
        if let Some(limit) = params.limit {
            req = req.query(&[("limit", limit.to_string())]);
        }
        if let Some(cursor) = &params.cursor {
            req = req.query(&[("cursor", cursor)]);
        }
        if let Some(user_id) = &params.user_id {
            req = req.query(&[("user_id", user_id)]);
        }
        if let Some(status) = &params.status {
            req = req.query(&[("status", status)]);
        }
        self.execute(req).await
    }

    /// GET /admin/v1/device-sessions
    pub async fn list_device_sessions(
        &self,
        params: &ListDeviceSessionsParams,
    ) -> Result<ListDeviceSessionsResponse> {
        let mut req = self.request(Method::GET, "/admin/v1/device-sessions");
        if let Some(limit) = params.limit {
            req = req.query(&[("limit", limit.to_string())]);
        }
        if let Some(cursor) = &params.cursor {
            req = req.query(&[("cursor", cursor)]);
        }
        if let Some(user_id) = &params.user_id {
            req = req.query(&[("user_id", user_id)]);
        }
        if let Some(status) = &params.status {
            req = req.query(&[("status", status)]);
        }
        self.execute(req).await
    }

    /// POST /admin/v1/sessions/:id/revoke
    pub async fn revoke_session(&self, id: &str) -> Result<()> {
        let req = self.request(Method::POST, &format!("/admin/v1/sessions/{}/revoke", id));
        self.execute_no_content(req).await
    }

    /// POST /admin/v1/device-sessions/:id/revoke
    pub async fn revoke_device_session(&self, id: &str) -> Result<()> {
        let req = self.request(
            Method::POST,
            &format!("/admin/v1/device-sessions/{}/revoke", id),
        );
        self.execute_no_content(req).await
    }

    /// GET /admin/v1/sysadmins
    pub async fn list_sysadmins(
        &self,
        params: &ListSysadminsParams,
    ) -> Result<ListSysadminsResponse> {
        let mut req = self.request(Method::GET, "/admin/v1/sysadmins");
        if let Some(limit) = params.limit {
            req = req.query(&[("limit", limit.to_string())]);
        }
        if let Some(cursor) = &params.cursor {
            req = req.query(&[("cursor", cursor)]);
        }
        self.execute(req).await
    }

    /// GET /admin/v1/settings
    pub async fn get_settings(&self) -> Result<SettingsResponse> {
        let req = self.request(Method::GET, "/admin/v1/settings");
        self.execute(req).await
    }

    /// POST /admin/v1/settings/waitlist
    pub async fn set_waitlist_enabled(&self, enabled: bool) -> Result<SettingsResponse> {
        let req = self
            .request(Method::POST, "/admin/v1/settings/waitlist")
            .json(&WaitlistSettingsRequest { enabled });
        self.execute(req).await
    }

    /// POST /admin/v1/users/release
    pub async fn release_user(&self, request: &ReleaseUserRequest) -> Result<ReleaseUserResponse> {
        let req = self
            .request(Method::POST, "/admin/v1/users/release")
            .json(request);
        self.execute(req).await
    }

    /// POST /admin/v1/users/invite
    pub async fn invite_user(&self, email: &str) -> Result<InviteUserResponse> {
        let req = self
            .request(Method::POST, "/admin/v1/users/invite")
            .json(&InviteUserRequest {
                email: email.to_string(),
            });
        self.execute(req).await
    }

    // --- Organization endpoints ---

    /// GET /admin/v1/orgs
    pub async fn list_orgs(&self, params: &ListOrgsParams) -> Result<ListOrgsResponse> {
        let mut req = self.request(Method::GET, "/admin/v1/orgs");
        if let Some(limit) = params.limit {
            req = req.query(&[("limit", limit.to_string())]);
        }
        if let Some(cursor) = &params.cursor {
            req = req.query(&[("cursor", cursor)]);
        }
        self.execute(req).await
    }

    /// GET /admin/v1/orgs/:id
    pub async fn get_org(&self, id: &str) -> Result<OrgDetail> {
        let req = self.request(Method::GET, &format!("/admin/v1/orgs/{}", id));
        self.execute(req).await
    }

    /// POST /admin/v1/orgs
    pub async fn create_org(&self, request: &CreateOrgRequest) -> Result<OrgDetail> {
        let req = self.request(Method::POST, "/admin/v1/orgs").json(request);
        self.execute(req).await
    }

    /// PATCH /admin/v1/orgs/:id
    pub async fn update_org(&self, id: &str, request: &UpdateOrgRequest) -> Result<()> {
        let req = self
            .request(Method::PATCH, &format!("/admin/v1/orgs/{}", id))
            .json(request);
        self.execute_no_content(req).await
    }

    /// DELETE /admin/v1/orgs/:id
    pub async fn delete_org(&self, id: &str) -> Result<()> {
        let req = self.request(Method::DELETE, &format!("/admin/v1/orgs/{}", id));
        self.execute_no_content(req).await
    }

    /// GET /admin/v1/orgs/:id/members
    pub async fn list_org_members(&self, id: &str) -> Result<ListMembersResponse> {
        let req = self.request(Method::GET, &format!("/admin/v1/orgs/{}/members", id));
        self.execute(req).await
    }

    /// POST /admin/v1/orgs/:id/members
    pub async fn add_org_member(&self, id: &str, request: &AddMemberRequest) -> Result<()> {
        let req = self
            .request(Method::POST, &format!("/admin/v1/orgs/{}/members", id))
            .json(request);
        self.execute_no_content(req).await
    }

    /// DELETE /admin/v1/orgs/:org_id/members/:user_id
    pub async fn remove_org_member(&self, org_id: &str, user_id: &str) -> Result<()> {
        let req = self.request(
            Method::DELETE,
            &format!("/admin/v1/orgs/{}/members/{}", org_id, user_id),
        );
        self.execute_no_content(req).await
    }

    /// PATCH /admin/v1/orgs/:org_id/members/:user_id
    pub async fn set_member_role(
        &self,
        org_id: &str,
        user_id: &str,
        request: &SetRoleRequest,
    ) -> Result<()> {
        let req = self
            .request(
                Method::PATCH,
                &format!("/admin/v1/orgs/{}/members/{}", org_id, user_id),
            )
            .json(request);
        self.execute_no_content(req).await
    }

    /// POST /admin/v1/orgs/:id/transfer
    pub async fn transfer_org_ownership(
        &self,
        id: &str,
        request: &TransferOwnershipRequest,
    ) -> Result<()> {
        let req = self
            .request(Method::POST, &format!("/admin/v1/orgs/{}/transfer", id))
            .json(request);
        self.execute_no_content(req).await
    }

    /// GET /admin/v1/orgs/:id/invites
    pub async fn list_org_invites(&self, id: &str) -> Result<ListOrgInvitesResponse> {
        let req = self.request(Method::GET, &format!("/admin/v1/orgs/{}/invites", id));
        self.execute(req).await
    }

    /// POST /admin/v1/orgs/:id/invites
    pub async fn create_org_invite(
        &self,
        id: &str,
        request: &CreateOrgInviteRequest,
    ) -> Result<CreateOrgInviteResponse> {
        let req = self
            .request(Method::POST, &format!("/admin/v1/orgs/{}/invites", id))
            .json(request);
        self.execute(req).await
    }

    /// DELETE /admin/v1/orgs/:org_id/invites/:invite_id
    pub async fn cancel_org_invite(&self, org_id: &str, invite_id: &str) -> Result<()> {
        let req = self.request(
            Method::DELETE,
            &format!("/admin/v1/orgs/{}/invites/{}", org_id, invite_id),
        );
        self.execute_no_content(req).await
    }
}

// Request parameter types.

/// Parameters for listing users.
#[derive(Debug, Default)]
pub struct ListUsersParams {
    /// Maximum number of users to return.
    pub limit: Option<i64>,
    /// Pagination cursor.
    pub cursor: Option<String>,
    /// Email filter.
    pub email: Option<String>,
}

/// Parameters for listing sessions.
#[derive(Debug, Default)]
pub struct ListSessionsParams {
    /// Maximum number of sessions to return.
    pub limit: Option<i64>,
    /// Pagination cursor.
    pub cursor: Option<String>,
    /// Filter by user ID.
    pub user_id: Option<String>,
    /// Filter by status.
    pub status: Option<String>,
}

/// Parameters for listing device sessions.
#[derive(Debug, Default)]
pub struct ListDeviceSessionsParams {
    /// Maximum number of sessions to return.
    pub limit: Option<i64>,
    /// Pagination cursor.
    pub cursor: Option<String>,
    /// Filter by user ID.
    pub user_id: Option<String>,
    /// Filter by status.
    pub status: Option<String>,
}

/// Parameters for listing sysadmins.
#[derive(Debug, Default)]
pub struct ListSysadminsParams {
    /// Maximum number of sysadmins to return.
    pub limit: Option<i64>,
    /// Pagination cursor.
    pub cursor: Option<String>,
}

/// Build an [`Error::Api`] from a failed response, extracting the structured
/// error code and message when the body parses and falling back to the status
/// otherwise.
fn api_error(status: StatusCode, body: &str) -> Error {
    let (code, message) = parse_json::<ApiErrorResponse>(body)
        .map(|e| (e.error.code, e.error.message))
        .unwrap_or_else(|_| ("unknown".to_string(), format!("HTTP {status}")));

    Error::Api {
        status: status.as_u16(),
        code,
        message,
    }
}

/// Truncate a response body for error reporting.
fn truncate_body(body: &str) -> String {
    const LIMIT: usize = 200;
    if body.len() <= LIMIT {
        return body.to_string();
    }

    // Back off to the nearest char boundary so slicing never splits a
    // multi-byte character (error bodies are arbitrary bytes-as-text).
    let end = (0..=LIMIT)
        .rev()
        .find(|&i| body.is_char_boundary(i))
        .unwrap_or(0);
    let mut result = body[..end].to_string();
    result.push_str("...");
    result
}

#[cfg(test)]
mod tests {
    //! Tests for client response helpers.

    use super::truncate_body;

    /// Short bodies are returned unchanged.
    #[test]
    fn keeps_short_body() {
        assert_eq!(truncate_body("hello"), "hello");
    }

    /// Long ASCII bodies are truncated with an ellipsis.
    #[test]
    fn truncates_long_ascii() {
        let body = "a".repeat(250);
        let out = truncate_body(&body);
        assert_eq!(out, format!("{}...", "a".repeat(200)));
    }

    /// Truncation never panics when a multi-byte char straddles the limit.
    #[test]
    fn truncates_on_char_boundary() {
        // Each '€' is three bytes; 100 of them is 300 bytes, so the 200-byte
        // limit lands mid-character. The naive byte slice would panic here.
        let body = "€".repeat(100);
        let out = truncate_body(&body);
        assert!(out.ends_with("..."));
        assert!(out.len() <= 203);
    }
}

/// Parameters for listing organizations.
#[derive(Debug, Default)]
pub struct ListOrgsParams {
    /// Maximum number of orgs to return.
    pub limit: Option<i64>,
    /// Pagination cursor.
    pub cursor: Option<String>,
}
