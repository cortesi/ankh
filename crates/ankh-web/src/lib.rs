#![warn(missing_docs)]

//! Shared Ankh web services, extractors, routers, hooks, and audit seams.

mod admin;
mod api;
mod auth;
mod errors;
mod hooks;
mod mail;
mod services;
mod state;
/// In-process router and application harness used by tests and the demo server.
pub mod test_support;

pub use admin::{
    AdminAudit, AdminAuditEvent, AdminAuditResult, AdminError, AdminResult, RequestContext,
    SysadminAuth, admin_router, emit_admin_audit,
};
pub use api::{ApiError, ApiResult, router};
pub use auth::{AuthSession, DeviceBearerSession, RequireActiveUser, RequireSession, bearer_token};
pub use hooks::{
    DeviceSessionsRevoked, FakeAuditSink, FakeHookRecorder, NamespaceDeleted,
    NamespaceStatusChanged, NoopProductHooks, OrgMemberRemoved, ProductHooks,
};
pub use mail::{MailState, MailTransport};
pub use state::{AnkhWebConfig, AnkhWebState, CookieConfig, DeviceAuthConfig};

#[cfg(test)]
mod tests {
    //! Smoke tests and ignored DB-backed router tests for the public web harness.

    use std::{future::Future, sync::Arc};

    use ankh_constants::{DEVICE_AUTH_EXCHANGE_RATE_PER_MINUTE, USER_LOGIN_RATE_PER_MINUTE};
    use ankh_db::{
        AnkhDbPool,
        test_support::{DEFAULT_POSTGRES_PORT, FreshDb, with_fresh_db},
    };
    use ankh_mail::{MailBranding, MailCatalog, PublicBaseUrl, RecordingMailer};
    use ankh_testdata::{
        ADMIN, ALICE, BOB, DEFAULT_ORG, PENDING_ORG_INVITE, SESSION_TTL, seed_identities,
    };
    use ankh_types::{DevicePlatform, DeviceSessionId, UserId};
    use async_trait::async_trait;
    use axum::{
        body::{Body, to_bytes},
        http::{
            HeaderMap, Method, Request, Response, StatusCode,
            header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, LOCATION, SET_COOKIE},
        },
    };
    use serde_json::{Value, json};
    use tokio::runtime::Builder as TokioRuntimeBuilder;
    use tower::ServiceExt;
    use url::Url;
    use uuid::Uuid;

    use super::{
        AdminAudit, AdminAuditEvent, AnkhWebConfig, AnkhWebState, CookieConfig,
        DeviceSessionsRevoked, FakeAuditSink, FakeHookRecorder, MailState, NamespaceDeleted,
        NamespaceStatusChanged, ProductHooks, test_support::TestAppHarness,
    };

    /// PKCE challenge for verifier `test-device-verifier`.
    const TEST_DEVICE_CHALLENGE: &str = "-h3fMaFx46QpbqSYNy5y8dFicxDubLWG6tjHbsu4rcw";
    /// PKCE verifier matching `TEST_DEVICE_CHALLENGE`.
    const TEST_DEVICE_VERIFIER: &str = "test-device-verifier";

    /// Product hook implementation that fails every hook call.
    #[derive(Debug, Default)]
    struct FailingHooks;

    #[async_trait]
    impl ProductHooks for FailingHooks {
        async fn on_org_member_removed(
            &self,
            _payload: super::OrgMemberRemoved,
        ) -> Result<(), String> {
            Err("hook failed".to_owned())
        }

        async fn on_namespace_suspended(
            &self,
            _payload: NamespaceStatusChanged,
        ) -> Result<(), String> {
            Err("hook failed".to_owned())
        }

        async fn on_namespace_reinstated(
            &self,
            _payload: NamespaceStatusChanged,
        ) -> Result<(), String> {
            Err("hook failed".to_owned())
        }

        async fn on_namespaces_deleted(
            &self,
            _payload: Vec<NamespaceDeleted>,
        ) -> Result<(), String> {
            Err("hook failed".to_owned())
        }

        async fn on_device_sessions_revoked(
            &self,
            _payload: DeviceSessionsRevoked,
        ) -> Result<(), String> {
            Err("hook failed".to_owned())
        }
    }

    /// Admin audit implementation that always fails.
    #[derive(Debug, Default)]
    struct FailingAdminAudit;

    #[async_trait]
    impl AdminAudit for FailingAdminAudit {
        async fn record(&self, _event: AdminAuditEvent) -> Result<(), String> {
            Err("audit failed".to_owned())
        }
    }

    /// Build a web state object with recording mail and test cookie behavior.
    fn test_state(pool: AnkhDbPool) -> AnkhWebState {
        let branding = MailBranding::new(
            "Ankh",
            PublicBaseUrl::new("http://127.0.0.1:52700").expect("valid base url"),
            "no-reply@example.com",
            "support@example.com",
        );
        let mail = MailState::new(RecordingMailer::new(), MailCatalog::default(), branding);
        AnkhWebState::with_config(
            pool,
            mail,
            AnkhWebConfig {
                cookie: CookieConfig {
                    secure: false,
                    ..CookieConfig::default()
                },
                ..AnkhWebConfig::default()
            },
        )
    }

    /// Run an async test body inside a fresh Ankh database.
    async fn with_seeded_harness<T, Run, RunFuture>(run: Run) -> ankh_db::Result<T>
    where
        Run: FnOnce(TestAppHarness, FreshDb) -> RunFuture,
        RunFuture: Future<Output = ankh_db::Result<T>>,
    {
        with_fresh_db(seed_identities, |fresh| async move {
            let state = test_state(fresh.pool().clone());
            run(TestAppHarness::new(state), fresh).await
        })
        .await
    }

    /// Run an async test body inside a fresh database with failing product hooks.
    async fn with_failing_hook_harness<T, Run, RunFuture>(run: Run) -> ankh_db::Result<T>
    where
        Run: FnOnce(TestAppHarness, FreshDb) -> RunFuture,
        RunFuture: Future<Output = ankh_db::Result<T>>,
    {
        with_fresh_db(seed_identities, |fresh| async move {
            let state = test_state(fresh.pool().clone()).with_hooks(Arc::new(FailingHooks));
            run(TestAppHarness::new(state), fresh).await
        })
        .await
    }

    /// Run an async test body inside a fresh database with recording admin sinks.
    async fn with_recording_admin_harness<T, Run, RunFuture>(run: Run) -> ankh_db::Result<T>
    where
        Run: FnOnce(TestAppHarness, FreshDb, FakeAuditSink, FakeHookRecorder) -> RunFuture,
        RunFuture: Future<Output = ankh_db::Result<T>>,
    {
        with_fresh_db(seed_identities, |fresh| async move {
            let audit = FakeAuditSink::new();
            let hooks = FakeHookRecorder::new();
            let state = test_state(fresh.pool().clone())
                .with_admin_audit(Arc::new(audit.clone()))
                .with_hooks(Arc::new(hooks.clone()));
            run(TestAppHarness::new(state), fresh, audit, hooks).await
        })
        .await
    }

    /// Build a JSON request with optional session cookie.
    fn json_request(uri: &str, body: &Value, cookie: Option<&str>) -> Request<Body> {
        request(
            Method::POST,
            uri,
            Body::from(body.to_string()),
            cookie,
            true,
        )
    }

    /// Build an empty request with optional session cookie.
    fn empty_request(method: Method, uri: &str, cookie: Option<&str>) -> Request<Body> {
        request(method, uri, Body::empty(), cookie, false)
    }

    /// Build an empty request with a bearer token.
    fn empty_bearer_request(method: Method, uri: &str, token: &str) -> Request<Body> {
        bearer_request(method, uri, Body::empty(), false, token)
    }

    /// Build a JSON request with a bearer token.
    fn json_bearer_request(method: Method, uri: &str, body: &Value, token: &str) -> Request<Body> {
        bearer_request(method, uri, Body::from(body.to_string()), true, token)
    }

    /// Build an HTTP request for router tests.
    fn request(
        method: Method,
        uri: &str,
        body: Body,
        cookie: Option<&str>,
        json_body: bool,
    ) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if json_body {
            builder = builder.header(CONTENT_TYPE, "application/json");
        }
        if let Some(cookie) = cookie {
            builder = builder.header(COOKIE, cookie);
        }
        builder.body(body).expect("test request")
    }

    /// Build an HTTP request carrying admin bearer authentication.
    fn bearer_request(
        method: Method,
        uri: &str,
        body: Body,
        json_body: bool,
        token: &str,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"));
        if json_body {
            builder = builder.header(CONTENT_TYPE, "application/json");
        }
        builder.body(body).expect("test request")
    }

    /// Read status, headers, and body from an Axum response.
    async fn read_response(response: Response<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body")
            .to_vec();
        (status, headers, body)
    }

    /// Parse a JSON response body.
    fn parse_json(body: &[u8]) -> Value {
        serde_json::from_slice(body).expect("response json")
    }

    /// Extract the first cookie pair from a Set-Cookie header.
    fn session_cookie(headers: &HeaderMap) -> String {
        headers
            .get(SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .expect("session cookie")
            .to_owned()
    }

    /// Log Alice in and return her session cookie.
    async fn login_cookie(harness: &TestAppHarness) -> String {
        let response = harness
            .router()
            .oneshot(json_request(
                "/api/v1/auth/login",
                &json!({ "email": ALICE.email, "password": ALICE.password }),
                None,
            ))
            .await
            .expect("login response");
        let (status, headers, body) = read_response(response).await;

        assert_eq!(
            status,
            StatusCode::OK,
            "body: {}",
            String::from_utf8_lossy(&body)
        );
        session_cookie(&headers)
    }

    /// Log the seeded sysadmin in and return an admin bearer token.
    async fn admin_token(harness: &TestAppHarness) -> String {
        let response = harness
            .router()
            .oneshot(json_request(
                "/admin/v1/auth/login",
                &json!({ "email": ADMIN.email, "password": ADMIN.password }),
                None,
            ))
            .await
            .expect("admin login response");
        let (status, _headers, body) = read_response(response).await;

        assert_eq!(
            status,
            StatusCode::OK,
            "body: {}",
            String::from_utf8_lossy(&body)
        );
        parse_json(&body)["token"]
            .as_str()
            .expect("admin token")
            .to_owned()
    }

    /// Extract a query parameter from an absolute URL.
    fn query_value(url: &str, key: &str) -> String {
        Url::parse(url)
            .expect("absolute url")
            .query_pairs()
            .find_map(|(name, value)| (name == key).then(|| value.into_owned()))
            .expect("query value")
    }

    /// Assert that a recorded label list contains an expected entry.
    fn assert_recorded(labels: &[String], expected: &str) {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing {expected}; recorded labels: {labels:?}"
        );
    }

    /// Proves audit sink observations can be inspected and drained.
    #[test]
    fn fake_audit_records_events() {
        let audit = FakeAuditSink::new();
        audit.record("admin.login");

        assert_eq!(audit.take_events(), vec!["admin.login"]);
        assert!(audit.take_events().is_empty());
    }

    /// Proves hook observations can be inspected and drained.
    #[test]
    fn fake_hooks_record_calls() {
        let hooks = FakeHookRecorder::new();
        hooks.record("device.revoked");

        assert_eq!(hooks.take_calls(), vec!["device.revoked"]);
        assert!(hooks.take_calls().is_empty());
    }

    /// Proves the in-process router harness can mount with recording mail and fake hooks.
    #[test]
    fn test_harness_builds_router_with_recording_mailer() {
        let params = format!("postgresql://localhost:{}/ankh-test", DEFAULT_POSTGRES_PORT);
        let pool = ankh_db::create_pg_pool(params).expect("test pool config parses");
        let branding = MailBranding::new(
            "Ankh",
            PublicBaseUrl::new("http://127.0.0.1:52700").expect("valid base url"),
            "no-reply@example.com",
            "support@example.com",
        );
        let mail = MailState::new(RecordingMailer::new(), MailCatalog::default(), branding);
        let hooks = FakeHookRecorder::new();
        let state = AnkhWebState::new(pool, mail).with_hooks(Arc::new(hooks));
        let harness = TestAppHarness::new(state);

        drop(harness.router());
        assert!(harness.state().take_hook_failures().is_empty());
    }

    /// Proves shared auth, org, and browser device-session routes work together.
    #[tokio::test(flavor = "current_thread")]
    async fn router_login_orgs_and_browser_device_session() -> ankh_db::Result<()> {
        with_seeded_harness(|harness, fresh| async move {
            let cookie = login_cookie(&harness).await;
            let response = harness
                .router()
                .oneshot(empty_request(Method::GET, "/api/v1/orgs", Some(&cookie)))
                .await
                .expect("org list response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let orgs = parse_json(&body);
            let org_names = orgs
                .as_array()
                .expect("org array")
                .iter()
                .filter_map(|org| org["name"].as_str())
                .collect::<Vec<_>>();
            assert!(org_names.contains(&ALICE.username));
            assert!(org_names.contains(&DEFAULT_ORG.name));

            let response = harness
                .router()
                .oneshot(empty_request(
                    Method::POST,
                    "/api/v1/device-sessions",
                    Some(&cookie),
                ))
                .await
                .expect("device session response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let payload = parse_json(&body);
            assert_eq!(payload["device_name"].as_str(), Some("Browser Player"));
            assert!(payload["token"].as_str().is_some());

            let db = fresh.get().await?;
            let user = db.get_user_by_email(ALICE.email).await?;
            let sessions = db.list_device_sessions_for_user(user.id).await?;
            assert!(sessions.iter().any(|session| {
                session.device_name == "Browser Player"
                    && session.platform == ankh_types::DevicePlatform::Web
            }));
            Ok(())
        })
        .await
    }

    /// Proves shared device authorization and token exchange routes compose.
    #[tokio::test(flavor = "current_thread")]
    async fn router_device_authorize_and_token_exchange() -> ankh_db::Result<()> {
        with_seeded_harness(|harness, _fresh| async move {
            let cookie = login_cookie(&harness).await;
            let authorize_uri = format!(
                "/api/v1/device/authorize?code_challenge={TEST_DEVICE_CHALLENGE}&\
                 state=state-123&redirect_port=41012&device_name=Desktop&platform=macos"
            );
            let response = harness
                .router()
                .oneshot(empty_request(Method::GET, &authorize_uri, Some(&cookie)))
                .await
                .expect("authorize response");
            let (status, headers, _body) = read_response(response).await;
            assert_eq!(status, StatusCode::FOUND);
            assert_eq!(
                headers
                    .get(CACHE_CONTROL)
                    .and_then(|value| value.to_str().ok()),
                Some("no-store")
            );
            let location = headers
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .expect("redirect location");
            assert!(location.starts_with("http://127.0.0.1:41012/callback?"));
            assert_eq!(query_value(location, "state"), "state-123");

            let code = query_value(location, "code");
            let response = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/device/token",
                    &json!({ "code": code, "code_verifier": TEST_DEVICE_VERIFIER }),
                    None,
                ))
                .await
                .expect("token response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let payload = parse_json(&body);
            assert_eq!(payload["device_name"].as_str(), Some("Desktop"));
            assert_eq!(payload["platform"].as_str(), Some("macos"));
            assert!(payload["token"].as_str().is_some());
            Ok(())
        })
        .await
    }

    /// Proves hook failures do not roll back a successful device-session revoke.
    #[tokio::test(flavor = "current_thread")]
    async fn device_revoke_hook_failure_is_recorded_after_db_effect() -> ankh_db::Result<()> {
        with_failing_hook_harness(|harness, fresh| async move {
            let cookie = login_cookie(&harness).await;
            let response = harness
                .router()
                .oneshot(empty_request(
                    Method::POST,
                    "/api/v1/device-sessions",
                    Some(&cookie),
                ))
                .await
                .expect("device session response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);

            let (user_id, session_id) = {
                let db = fresh.get().await?;
                let user = db.get_user_by_email(ALICE.email).await?;
                let session_id = db
                    .list_device_sessions_for_user(user.id)
                    .await?
                    .first()
                    .map(|session| session.id)
                    .expect("created device session");
                (user.id, session_id)
            };
            assert!(parse_json(&body)["token"].as_str().is_some());

            let response = harness
                .router()
                .oneshot(empty_request(
                    Method::DELETE,
                    format!("/api/v1/device-sessions/{session_id}").as_str(),
                    Some(&cookie),
                ))
                .await
                .expect("delete device session response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let db = fresh.get().await?;
            let sessions = db.list_device_sessions_for_user(user_id).await?;
            assert!(sessions.iter().all(|session| session.id != session_id));
            assert_eq!(
                harness.state().take_hook_failures(),
                vec!["on_device_sessions_revoked: hook failed"]
            );
            Ok(())
        })
        .await
    }

    /// Proves shared admin login, bearer auth, and user listing compose.
    #[tokio::test(flavor = "current_thread")]
    async fn admin_login_whoami_and_users_list() -> ankh_db::Result<()> {
        with_recording_admin_harness(|harness, _fresh, audit, _hooks| async move {
            let token = admin_token(&harness).await;
            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::GET,
                    "/admin/v1/sysadmins/me",
                    token.as_str(),
                ))
                .await
                .expect("whoami response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(parse_json(&body)["sysadmin"]["email"], ADMIN.email);

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::GET,
                    "/admin/v1/users",
                    token.as_str(),
                ))
                .await
                .expect("users response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let users = parse_json(&body);
            let emails = users["users"]
                .as_array()
                .expect("users array")
                .iter()
                .filter_map(|user| user["email"].as_str())
                .collect::<Vec<_>>();
            assert!(emails.contains(&ALICE.email));

            assert_eq!(audit.take_events(), vec!["admin.login:sysadmin:success"]);
            Ok(())
        })
        .await
    }

    /// Proves admin user deletion dispatches concrete device-session and namespace hooks.
    #[tokio::test(flavor = "current_thread")]
    async fn admin_user_delete_dispatches_device_and_namespace_hooks() -> ankh_db::Result<()> {
        with_recording_admin_harness(|harness, fresh, audit, hooks| async move {
            let token = admin_token(&harness).await;
            let (bob_id, session_id) = {
                let db = fresh.get().await?;
                let bob = db.get_user_by_email(BOB.email).await?;
                let created = db
                    .create_device_session(
                        bob.id,
                        "Bob Test Device",
                        &DevicePlatform::Macos,
                        SESSION_TTL,
                    )
                    .await?;
                (bob.id.to_string(), created.session.id.to_string())
            };

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::DELETE,
                    format!("/admin/v1/users/{bob_id}").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("user delete response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::NO_CONTENT,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let hook_calls = hooks.take_calls();
            assert_recorded(
                &hook_calls,
                format!("device_sessions_revoked:{bob_id}:{session_id}").as_str(),
            );
            assert_recorded(&hook_calls, "namespaces_deleted:1");
            let audit_events = audit.take_events();
            assert_recorded(&audit_events, "admin.login:sysadmin:success");
            assert_recorded(&audit_events, "user.delete:user:success");
            Ok(())
        })
        .await
    }

    /// Proves shared admin session, settings, org, member, transfer, and invite routes compose.
    #[tokio::test(flavor = "current_thread")]
    async fn admin_sessions_settings_orgs_members_transfers_and_invites() -> ankh_db::Result<()> {
        with_recording_admin_harness(|harness, fresh, audit, hooks| async move {
            let token = admin_token(&harness).await;
            let (alice_id, bob_id, default_org_id) = {
                let db = fresh.get().await?;
                let alice = db.get_user_by_email(ALICE.email).await?;
                let bob = db.get_user_by_email(BOB.email).await?;
                let org = db.get_org_by_name(DEFAULT_ORG.name).await?;
                (alice.id.to_string(), bob.id.to_string(), org.id.to_string())
            };

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::GET,
                    format!("/admin/v1/sessions?user_id={alice_id}").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("sessions response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let sessions = parse_json(&body);
            let session_id = sessions["sessions"]
                .as_array()
                .expect("sessions array")
                .first()
                .and_then(|session| session["id"].as_str())
                .expect("session id")
                .to_owned();

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::POST,
                    format!("/admin/v1/sessions/{session_id}/revoke").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("session revoke response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::NO_CONTENT,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::GET,
                    "/admin/v1/settings",
                    token.as_str(),
                ))
                .await
                .expect("settings response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            assert!(parse_json(&body)["waitlist_enabled"].is_boolean());

            let response = harness
                .router()
                .oneshot(json_bearer_request(
                    Method::POST,
                    "/admin/v1/settings/waitlist",
                    &json!({ "enabled": true }),
                    token.as_str(),
                ))
                .await
                .expect("waitlist response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(parse_json(&body)["waitlist_enabled"], true);

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::GET,
                    format!("/admin/v1/orgs/{default_org_id}").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("org detail response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let org = parse_json(&body);
            assert_eq!(org["name"], DEFAULT_ORG.name);
            assert_eq!(org["namespace_status"], "active");

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::GET,
                    format!("/admin/v1/orgs/{default_org_id}/invites").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("invites response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let invites = parse_json(&body);
            assert!(
                invites["invites"]
                    .as_array()
                    .expect("invites array")
                    .iter()
                    .any(|invite| invite["email"] == PENDING_ORG_INVITE.email)
            );

            let response = harness
                .router()
                .oneshot(json_bearer_request(
                    Method::POST,
                    "/admin/v1/orgs",
                    &json!({
                        "name": "admin-extra-org",
                        "display_name": "Admin Extra Org",
                        "owner_id": alice_id,
                    }),
                    token.as_str(),
                ))
                .await
                .expect("org create response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::CREATED,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            let extra_org_id = parse_json(&body)["id"].as_str().expect("org id").to_owned();

            let response = harness
                .router()
                .oneshot(json_bearer_request(
                    Method::POST,
                    format!("/admin/v1/orgs/{extra_org_id}/members").as_str(),
                    &json!({ "user_id": bob_id, "role": "member" }),
                    token.as_str(),
                ))
                .await
                .expect("member add response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::CREATED,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let response = harness
                .router()
                .oneshot(json_bearer_request(
                    Method::PATCH,
                    format!("/admin/v1/orgs/{extra_org_id}/members/{bob_id}").as_str(),
                    &json!({ "role": "admin" }),
                    token.as_str(),
                ))
                .await
                .expect("member role response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::NO_CONTENT,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let response = harness
                .router()
                .oneshot(json_bearer_request(
                    Method::POST,
                    format!("/admin/v1/orgs/{extra_org_id}/transfer").as_str(),
                    &json!({ "new_owner_id": bob_id }),
                    token.as_str(),
                ))
                .await
                .expect("transfer response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::NO_CONTENT,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::DELETE,
                    format!("/admin/v1/orgs/{extra_org_id}/members/{alice_id}").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("member remove response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::NO_CONTENT,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let response = harness
                .router()
                .oneshot(json_bearer_request(
                    Method::POST,
                    format!("/admin/v1/orgs/{extra_org_id}/invites").as_str(),
                    &json!({ "email": "new-invite@example.com" }),
                    token.as_str(),
                ))
                .await
                .expect("invite create response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::CREATED,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            let invite_id = parse_json(&body)["id"]
                .as_str()
                .expect("invite id")
                .to_owned();

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::DELETE,
                    format!("/admin/v1/orgs/{extra_org_id}/invites/{invite_id}").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("invite cancel response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::NO_CONTENT,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let response = harness
                .router()
                .oneshot(json_bearer_request(
                    Method::POST,
                    "/admin/v1/orgs",
                    &json!({
                        "name": "admin-delete-org",
                        "display_name": null,
                        "owner_id": alice_id,
                    }),
                    token.as_str(),
                ))
                .await
                .expect("delete-org create response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::CREATED,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            let delete_org_id = parse_json(&body)["id"].as_str().expect("org id").to_owned();

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::DELETE,
                    format!("/admin/v1/orgs/{delete_org_id}").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("org delete response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::NO_CONTENT,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            let hook_calls = hooks.take_calls();
            assert_recorded(
                &hook_calls,
                format!("org_member_removed:admin-extra-org:{alice_id}").as_str(),
            );
            assert_recorded(&hook_calls, "namespaces_deleted:1");

            let audit_events = audit.take_events();
            for expected in [
                "admin.login:sysadmin:success",
                "session.revoke:session:success",
                "settings.waitlist.update:settings:success",
                "org.create:org:success",
                "org.member.add:org_member:success",
                "org.member.role:org_member:success",
                "org.transfer:org:success",
                "org.member.remove:org_member:success",
                "org.invite.create:org_invite:success",
                "org.invite.cancel:org_invite:success",
                "org.delete:org:success",
            ] {
                assert_recorded(&audit_events, expected);
            }
            Ok(())
        })
        .await
    }

    /// Proves admin errors use the shared envelope and audit sink failures stay best-effort.
    #[tokio::test(flavor = "current_thread")]
    async fn admin_error_envelope_and_audit_sink_failure() -> ankh_db::Result<()> {
        with_fresh_db(seed_identities, |fresh| async move {
            let state =
                test_state(fresh.pool().clone()).with_admin_audit(Arc::new(FailingAdminAudit));
            let harness = TestAppHarness::new(state);
            let token = admin_token(&harness).await;
            assert_eq!(
                harness.state().take_audit_failures(),
                vec!["admin_audit: audit failed"]
            );

            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::GET,
                    "/admin/v1/users/not-a-user-id",
                    token.as_str(),
                ))
                .await
                .expect("error response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            let error = parse_json(&body);
            assert_eq!(error["error"]["code"], "bad_request");
            assert_eq!(error["error"]["message"], "invalid user id format");

            let response = harness
                .router()
                .oneshot(json_bearer_request(
                    Method::POST,
                    "/admin/v1/settings/waitlist",
                    &json!({ "enabled": true }),
                    token.as_str(),
                ))
                .await
                .expect("waitlist response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            assert_eq!(
                harness.state().take_audit_failures(),
                vec!["admin_audit: audit failed"]
            );
            Ok(())
        })
        .await
    }

    /// Proves shared admin device-session revocation emits product hooks and audit.
    #[tokio::test(flavor = "current_thread")]
    async fn admin_device_session_revoke_dispatches_hook() -> ankh_db::Result<()> {
        with_recording_admin_harness(|harness, fresh, audit, hooks| async move {
            let cookie = login_cookie(&harness).await;
            let token = admin_token(&harness).await;
            let response = harness
                .router()
                .oneshot(empty_request(
                    Method::POST,
                    "/api/v1/device-sessions",
                    Some(&cookie),
                ))
                .await
                .expect("device session response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            assert!(parse_json(&body)["token"].as_str().is_some());

            let (user_id, session_id) = {
                let db = fresh.get().await?;
                let user = db.get_user_by_email(ALICE.email).await?;
                let session_id = db
                    .list_device_sessions_for_user(user.id)
                    .await?
                    .first()
                    .map(|session| session.id)
                    .expect("created device session");
                (user.id, session_id)
            };
            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::POST,
                    format!("/admin/v1/device-sessions/{session_id}/revoke").as_str(),
                    token.as_str(),
                ))
                .await
                .expect("admin revoke response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::NO_CONTENT,
                "body: {}",
                String::from_utf8_lossy(&body)
            );

            assert_eq!(
                hooks.take_calls(),
                vec![format!("device_sessions_revoked:{user_id}:{session_id}")]
            );
            assert_eq!(
                audit.take_events(),
                vec![
                    "admin.login:sysadmin:success",
                    "device_session.revoke:device_session:success",
                ]
            );
            Ok(())
        })
        .await
    }

    /// Proves shared namespace admin mutations emit product hooks.
    #[tokio::test(flavor = "current_thread")]
    async fn admin_namespace_suspend_and_reinstate_dispatch_hooks() -> ankh_db::Result<()> {
        with_recording_admin_harness(|harness, fresh, audit, hooks| async move {
            let token = admin_token(&harness).await;
            let namespace_id = {
                let db = fresh.get().await?;
                db.get_org_by_name(DEFAULT_ORG.name).await?.namespace_id
            };

            let suspend_uri = format!("/admin/v1/namespaces/{namespace_id}/suspend");
            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::POST,
                    suspend_uri.as_str(),
                    token.as_str(),
                ))
                .await
                .expect("suspend response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let payload = parse_json(&body);
            assert_eq!(payload["status"], "suspended");
            let suspended_gen = payload["gen"].as_i64().expect("suspend generation");

            let reinstate_uri = format!("/admin/v1/namespaces/{namespace_id}/reinstate");
            let response = harness
                .router()
                .oneshot(empty_bearer_request(
                    Method::POST,
                    reinstate_uri.as_str(),
                    token.as_str(),
                ))
                .await
                .expect("reinstate response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(status, StatusCode::OK);
            let payload = parse_json(&body);
            assert_eq!(payload["status"], "active");
            assert_eq!(payload["gen"].as_i64(), Some(suspended_gen + 1));

            assert_eq!(
                hooks.take_calls(),
                vec![
                    format!("namespace_suspended:{}:{suspended_gen}", DEFAULT_ORG.name),
                    format!(
                        "namespace_reinstated:{}:{}",
                        DEFAULT_ORG.name,
                        suspended_gen + 1
                    ),
                ]
            );
            assert_eq!(
                audit.take_events(),
                vec![
                    "admin.login:sysadmin:success",
                    "namespace.suspend:namespace:success",
                    "namespace.reinstate:namespace:success",
                ]
            );
            Ok(())
        })
        .await
    }

    /// Proves all hook-failure payload builders remain constructible in tests.
    #[test]
    fn failing_hooks_is_a_complete_product_hook_implementation() {
        let payload = DeviceSessionsRevoked {
            user_id: UserId(Uuid::nil()),
            session_ids: vec![DeviceSessionId(Uuid::nil())],
        };
        let _hook = FailingHooks;

        assert_eq!(payload.session_ids.len(), 1);
    }

    /// Drive an async test body on a current-thread runtime.
    fn block_on<F: Future>(future: F) -> F::Output {
        TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime")
            .block_on(future)
    }

    /// Run a test body with a harness whose mailer is a caller-visible recording sink.
    async fn with_mail_harness<T, Run, RunFuture>(run: Run) -> ankh_db::Result<T>
    where
        Run: FnOnce(TestAppHarness, FreshDb, RecordingMailer) -> RunFuture,
        RunFuture: Future<Output = ankh_db::Result<T>>,
    {
        with_fresh_db(seed_identities, |fresh| async move {
            let mailer = RecordingMailer::new();
            let branding = MailBranding::new(
                "Ankh",
                PublicBaseUrl::new("http://127.0.0.1:52700").expect("valid base url"),
                "no-reply@example.com",
                "support@example.com",
            );
            let mail = MailState::new(mailer.clone(), MailCatalog::default(), branding);
            let state = AnkhWebState::with_config(
                fresh.pool().clone(),
                mail,
                AnkhWebConfig {
                    cookie: CookieConfig {
                        secure: false,
                        ..CookieConfig::default()
                    },
                    ..AnkhWebConfig::default()
                },
            );
            run(TestAppHarness::new(state), fresh, mailer).await
        })
        .await
    }

    /// Extract the `token` query-parameter value embedded in a captured email body.
    fn token_from_email(body: &str) -> String {
        let marker = "token=";
        let start = body.find(marker).expect("email body contains a token") + marker.len();
        body[start..]
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
            .collect()
    }

    /// Signup of an active account auto-logs-in and sends a verification email.
    #[test]
    fn signup_active_sends_verification_mail() -> ankh_db::Result<()> {
        block_on(with_mail_harness(|harness, _fresh, mailer| async move {
            let response = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/signup",
                    &json!({
                        "username": "carol",
                        "email": "carol@example.com",
                        "password": "carol-password",
                        "invite_token": null,
                        "org_invite_token": null
                    }),
                    None,
                ))
                .await
                .expect("signup response");
            let (status, headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            assert!(headers.contains_key(SET_COOKIE), "signup should auto-login");
            let user = parse_json(&body);
            assert_eq!(user["waitlisted"], json!(false));

            let sent = mailer.take_sent();
            assert!(
                sent.iter().any(|email| email.to == "carol@example.com"),
                "signup should send a verification email"
            );
            Ok(())
        }))
    }

    /// Signup while waitlist mode is enabled marks the account waitlisted.
    #[test]
    fn signup_is_waitlisted_when_waitlist_enabled() -> ankh_db::Result<()> {
        block_on(with_mail_harness(|harness, fresh, _mailer| async move {
            fresh.get().await?.set_waitlist_enabled(true).await?;

            let response = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/signup",
                    &json!({
                        "username": "dave",
                        "email": "dave@example.com",
                        "password": "dave-password",
                        "invite_token": null,
                        "org_invite_token": null
                    }),
                    None,
                ))
                .await
                .expect("signup response");
            let (status, _headers, body) = read_response(response).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            assert_eq!(parse_json(&body)["waitlisted"], json!(true));
            Ok(())
        }))
    }

    /// The verification token emailed at signup verifies the account.
    #[test]
    fn verify_email_with_emailed_token() -> ankh_db::Result<()> {
        block_on(with_mail_harness(|harness, _fresh, mailer| async move {
            let signup = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/signup",
                    &json!({
                        "username": "erin",
                        "email": "erin@example.com",
                        "password": "erin-password",
                        "invite_token": null,
                        "org_invite_token": null
                    }),
                    None,
                ))
                .await
                .expect("signup response");
            assert_eq!(read_response(signup).await.0, StatusCode::OK);

            let email = mailer
                .take_sent()
                .into_iter()
                .find(|email| email.to == "erin@example.com")
                .expect("verification email");
            let token = token_from_email(&email.text_body);

            let verify = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/verify-email",
                    &json!({ "token": token }),
                    None,
                ))
                .await
                .expect("verify response");
            assert_eq!(read_response(verify).await.0, StatusCode::OK);

            // Logging in now reports the address as verified.
            let login = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/login",
                    &json!({ "email": "erin@example.com", "password": "erin-password" }),
                    None,
                ))
                .await
                .expect("login response");
            let (status, _headers, body) = read_response(login).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(parse_json(&body)["email_verified"], json!(true));
            Ok(())
        }))
    }

    /// The forgot/validate/reset password flow rotates credentials and emails a token.
    #[test]
    fn password_reset_flow_updates_credentials() -> ankh_db::Result<()> {
        block_on(with_mail_harness(|harness, _fresh, mailer| async move {
            let forgot = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/forgot-password",
                    &json!({ "email": ALICE.email }),
                    None,
                ))
                .await
                .expect("forgot response");
            assert_eq!(read_response(forgot).await.0, StatusCode::OK);

            let email = mailer
                .take_sent()
                .into_iter()
                .find(|email| email.to == ALICE.email)
                .expect("reset email");
            let token = token_from_email(&email.text_body);

            let validate = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/validate-reset-token",
                    &json!({ "token": token }),
                    None,
                ))
                .await
                .expect("validate response");
            assert_eq!(read_response(validate).await.0, StatusCode::OK);

            let reset = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/reset-password",
                    &json!({ "token": token, "new_password": "alice-new-password" }),
                    None,
                ))
                .await
                .expect("reset response");
            assert_eq!(read_response(reset).await.0, StatusCode::OK);

            // The new password works and the old one no longer does.
            let new_login = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/login",
                    &json!({ "email": ALICE.email, "password": "alice-new-password" }),
                    None,
                ))
                .await
                .expect("new login response");
            assert_eq!(read_response(new_login).await.0, StatusCode::OK);

            let old_login = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/login",
                    &json!({ "email": ALICE.email, "password": ALICE.password }),
                    None,
                ))
                .await
                .expect("old login response");
            assert_eq!(read_response(old_login).await.0, StatusCode::UNAUTHORIZED);
            Ok(())
        }))
    }

    /// Resending verification for a logged-in unverified user sends a fresh email.
    #[test]
    fn resend_verification_sends_mail() -> ankh_db::Result<()> {
        block_on(with_mail_harness(|harness, _fresh, mailer| async move {
            let login = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/auth/login",
                    &json!({ "email": BOB.email, "password": BOB.password }),
                    None,
                ))
                .await
                .expect("login response");
            let (status, headers, _body) = read_response(login).await;
            assert_eq!(status, StatusCode::OK);
            let cookie = session_cookie(&headers);

            let resend = harness
                .router()
                .oneshot(empty_request(
                    Method::POST,
                    "/api/v1/auth/resend-verification",
                    Some(&cookie),
                ))
                .await
                .expect("resend response");
            assert_eq!(read_response(resend).await.0, StatusCode::OK);

            assert!(
                mailer.take_sent().iter().any(|email| email.to == BOB.email),
                "resend should send a verification email to the user"
            );
            Ok(())
        }))
    }

    /// Repeated failed logins for one email eventually hit the per-email rate limit.
    #[test]
    fn login_rate_limit_returns_too_many_requests() -> ankh_db::Result<()> {
        block_on(with_seeded_harness(|harness, _fresh| async move {
            let mut saw_rate_limit = false;
            for _ in 0..(USER_LOGIN_RATE_PER_MINUTE + 1) {
                let response = harness
                    .router()
                    .oneshot(json_request(
                        "/api/v1/auth/login",
                        &json!({ "email": "ghost@example.com", "password": "nope" }),
                        None,
                    ))
                    .await
                    .expect("login response");
                if response.status() == StatusCode::TOO_MANY_REQUESTS {
                    saw_rate_limit = true;
                    break;
                }
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
            assert!(saw_rate_limit, "login should be rate limited");
            Ok(())
        }))
    }

    /// A user can create an org, owns it, appears in its members, and cannot leave as owner.
    #[test]
    fn public_org_create_membership_and_owner_cannot_leave() -> ankh_db::Result<()> {
        block_on(with_seeded_harness(|harness, _fresh| async move {
            let cookie = login_cookie(&harness).await;

            let create = harness
                .router()
                .oneshot(json_request(
                    "/api/v1/orgs",
                    &json!({ "name": "acme", "display_name": "Acme Inc" }),
                    Some(&cookie),
                ))
                .await
                .expect("create org response");
            let (status, _headers, body) = read_response(create).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            let org = parse_json(&body);
            assert_eq!(org["name"], json!("acme"));
            assert_eq!(org["role"], json!("owner"));
            let org_id = org["id"].as_str().expect("org id").to_owned();

            // The creator is a member with the owner role.
            let members = harness
                .router()
                .oneshot(empty_request(
                    Method::GET,
                    &format!("/api/v1/orgs/{org_id}/members"),
                    Some(&cookie),
                ))
                .await
                .expect("members response");
            let (status, _headers, body) = read_response(members).await;
            assert_eq!(status, StatusCode::OK);
            let members = parse_json(&body);
            assert_eq!(members.as_array().expect("members array").len(), 1);
            assert_eq!(members[0]["email"], json!(ALICE.email));

            // The sole owner cannot leave their own org.
            let leave = harness
                .router()
                .oneshot(json_request(
                    &format!("/api/v1/orgs/{org_id}/leave"),
                    &json!({}),
                    Some(&cookie),
                ))
                .await
                .expect("leave response");
            assert!(
                read_response(leave).await.0.is_client_error(),
                "the owner must not be able to leave their org"
            );
            Ok(())
        }))
    }

    /// Sign up a new active account and return its session cookie.
    async fn signup_cookie(
        harness: &TestAppHarness,
        email: &str,
        username: &str,
        password: &str,
    ) -> String {
        let response = harness
            .router()
            .oneshot(json_request(
                "/api/v1/auth/signup",
                &json!({
                    "username": username,
                    "email": email,
                    "password": password,
                    "invite_token": null,
                    "org_invite_token": null
                }),
                None,
            ))
            .await
            .expect("signup response");
        let (status, headers, body) = read_response(response).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "signup body: {}",
            String::from_utf8_lossy(&body)
        );
        session_cookie(&headers)
    }

    /// Owner invites, cancels, removes a member, and an invitee accepts the seeded invite.
    #[test]
    fn public_org_invite_cancel_remove_and_accept() -> ankh_db::Result<()> {
        block_on(with_seeded_harness(|harness, fresh| async move {
            let (org_id, bob_id) = {
                let db = fresh.get().await?;
                (
                    db.get_org_by_name(DEFAULT_ORG.name).await?.id.to_string(),
                    db.get_user_by_email(BOB.email).await?.id.to_string(),
                )
            };
            let owner = login_cookie(&harness).await; // Alice owns the seeded org.

            // Create an invite and read back its id.
            let invite = harness
                .router()
                .oneshot(json_request(
                    &format!("/api/v1/orgs/{org_id}/invites"),
                    &json!({ "invite_email": "newbie@example.com" }),
                    Some(&owner),
                ))
                .await
                .expect("invite response");
            let (status, _headers, body) = read_response(invite).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            let invite_id = parse_json(&body)["id"]
                .as_str()
                .expect("invite id")
                .to_owned();

            // Cancel that invite.
            let cancel = harness
                .router()
                .oneshot(empty_request(
                    Method::DELETE,
                    &format!("/api/v1/orgs/{org_id}/invites/{invite_id}"),
                    Some(&owner),
                ))
                .await
                .expect("cancel response");
            assert_eq!(read_response(cancel).await.0, StatusCode::OK);

            // Remove the seeded member Bob.
            let remove = harness
                .router()
                .oneshot(empty_request(
                    Method::DELETE,
                    &format!("/api/v1/orgs/{org_id}/members/{bob_id}"),
                    Some(&owner),
                ))
                .await
                .expect("remove response");
            assert_eq!(read_response(remove).await.0, StatusCode::OK);

            // A freshly signed-up invitee accepts the seeded pending org invite.
            let invitee = signup_cookie(
                &harness,
                PENDING_ORG_INVITE.email,
                "invitee",
                "invitee-pass",
            )
            .await;
            let accept = harness
                .router()
                .oneshot(empty_request(
                    Method::POST,
                    &format!("/api/v1/org-invites/{}/accept", PENDING_ORG_INVITE.token),
                    Some(&invitee),
                ))
                .await
                .expect("accept response");
            let (status, _headers, body) = read_response(accept).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "body: {}",
                String::from_utf8_lossy(&body)
            );
            assert_eq!(parse_json(&body)["name"], json!(DEFAULT_ORG.name));
            Ok(())
        }))
    }

    /// A waitlisted user signs up successfully but is blocked (403) from product
    /// routes, while still reaching endpoints they need (e.g. `me`).
    #[test]
    fn waitlisted_user_is_forbidden_from_product_routes() -> ankh_db::Result<()> {
        block_on(with_seeded_harness(|harness, fresh| async move {
            fresh.get().await?.set_waitlist_enabled(true).await?;

            // Signup succeeds and auto-logs-in even while waitlisted.
            let cookie =
                signup_cookie(&harness, "wanda@example.com", "wanda", "wanda-password").await;

            // Product routes reject the waitlisted session with 403.
            let orgs = harness
                .router()
                .oneshot(empty_request(Method::GET, "/api/v1/orgs", Some(&cookie)))
                .await
                .expect("orgs response");
            assert_eq!(read_response(orgs).await.0, StatusCode::FORBIDDEN);

            let device = harness
                .router()
                .oneshot(empty_request(
                    Method::POST,
                    "/api/v1/device-sessions",
                    Some(&cookie),
                ))
                .await
                .expect("device session response");
            assert_eq!(read_response(device).await.0, StatusCode::FORBIDDEN);

            // Endpoints a waitlisted user still needs remain reachable.
            let me = harness
                .router()
                .oneshot(empty_request(Method::GET, "/api/v1/auth/me", Some(&cookie)))
                .await
                .expect("me response");
            assert_eq!(read_response(me).await.0, StatusCode::OK);
            Ok(())
        }))
    }

    /// Device token exchange is rate limited per client IP.
    #[test]
    fn device_token_exchange_is_rate_limited_per_ip() -> ankh_db::Result<()> {
        block_on(with_seeded_harness(|harness, _fresh| async move {
            let mut saw_rate_limit = false;
            for _ in 0..(DEVICE_AUTH_EXCHANGE_RATE_PER_MINUTE + 1) {
                let request = Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/device/token")
                    .header(CONTENT_TYPE, "application/json")
                    .header("x-forwarded-for", "203.0.113.7")
                    .body(Body::from(
                        json!({ "code": "bogus", "code_verifier": "bogus" }).to_string(),
                    ))
                    .expect("device token request");
                let response = harness
                    .router()
                    .oneshot(request)
                    .await
                    .expect("device token response");
                if response.status() == StatusCode::TOO_MANY_REQUESTS {
                    saw_rate_limit = true;
                    break;
                }
            }
            assert!(
                saw_rate_limit,
                "device token exchange should be rate limited per IP"
            );
            Ok(())
        }))
    }
}
