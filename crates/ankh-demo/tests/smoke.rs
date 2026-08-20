//! Cross-layer smoke test for the demo stack: real router + DB + dev mailer.

#[cfg(test)]
mod tests {
    //! Drives the merged public/admin router in-process (via `tower`'s `oneshot`) against a fresh
    //! database, asserting the demo-specific wiring: HTTP-friendly (non-`Secure`) session cookies
    //! and a [`ankh_mail::DevMailer`] that persists captured mail to disk. The admin login
    //! exercises the admin router that `ankh-cli` targets over HTTP.

    use ankh_db::{Result as DbResult, test_support::with_fresh_db};
    use ankh_demo::build_app;
    use ankh_mail::read_all_dev_mail;
    use ankh_testdata::{ADMIN, ALICE, seed_identities};
    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
    };
    use tower::ServiceExt;

    /// Build a JSON request for the in-process router.
    fn json_request(method: Method, uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .expect("build request")
    }

    #[tokio::test(flavor = "current_thread")]
    async fn demo_stack_serves_auth_admin_and_dev_mail() -> DbResult<()> {
        let mail_dir = tempfile::tempdir().expect("mail tempdir");
        let mail_path = mail_dir.path().to_path_buf();

        with_fresh_db(seed_identities, |fresh| async move {
            let app = build_app(
                fresh.pool().clone(),
                "http://127.0.0.1:8080",
                mail_path.clone(),
            )
            .expect("build demo app");

            // Public login as the seeded verified user succeeds and sets a non-Secure cookie,
            // proving the demo's HTTP-localhost cookie configuration.
            let login = app
                .clone()
                .oneshot(json_request(
                    Method::POST,
                    "/api/v1/auth/login",
                    &format!(
                        r#"{{"email":"{}","password":"{}"}}"#,
                        ALICE.email, ALICE.password
                    ),
                ))
                .await
                .expect("login response");
            assert_eq!(login.status(), StatusCode::OK);
            let cookie = login
                .headers()
                .get(header::SET_COOKIE)
                .and_then(|value| value.to_str().ok())
                .expect("session cookie");
            assert!(
                !cookie.contains("Secure"),
                "demo cookies must omit Secure for HTTP localhost: {cookie}"
            );

            // A password-reset request renders and sends mail through the DevMailer, which
            // persists it to the configured directory.
            let forgot = app
                .clone()
                .oneshot(json_request(
                    Method::POST,
                    "/api/v1/auth/forgot-password",
                    &format!(r#"{{"email":"{}"}}"#, ALICE.email),
                ))
                .await
                .expect("forgot-password response");
            assert_eq!(forgot.status(), StatusCode::OK);

            let mail = read_all_dev_mail(&mail_path).expect("read dev mail");
            assert!(
                mail.iter().any(|email| email.to == ALICE.email),
                "expected a captured reset email addressed to {}",
                ALICE.email
            );

            // The admin router (the surface ankh-cli drives over HTTP) authenticates the seeded
            // sysadmin and returns a token.
            let admin_login = app
                .clone()
                .oneshot(json_request(
                    Method::POST,
                    "/admin/v1/auth/login",
                    &format!(
                        r#"{{"email":"{}","password":"{}"}}"#,
                        ADMIN.email, ADMIN.password
                    ),
                ))
                .await
                .expect("admin login response");
            assert_eq!(admin_login.status(), StatusCode::OK);
            let body = to_bytes(admin_login.into_body(), usize::MAX)
                .await
                .expect("admin login body");
            assert!(
                String::from_utf8_lossy(&body).contains("token"),
                "admin login should return a token"
            );

            Ok(())
        })
        .await
    }
}
