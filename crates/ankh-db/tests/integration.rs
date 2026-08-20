//! DB-layer integration tests exercising `AnkhDb` against a fresh Postgres database.

#[cfg(test)]
mod tests {
    //! These run against the workspace Postgres (`cargo xtask db start`); `cargo xtask test`
    //! provisions it automatically.

    use std::{collections::HashSet, future::Future, time::Duration};

    use ankh_db::{
        ANKH_SCHEMA_VERSION, AnkhDb, AnkhDbPool, DeviceAuthGrantRequest, DevicePlatform, Error,
        OrgRole, Result as DbResult, TokenKind, UserId, test_support::with_fresh_db,
    };
    use tokio::runtime::Builder as TokioRuntimeBuilder;

    /// PKCE verifier and its matching S256 challenge (shared with the web router tests).
    const PKCE_VERIFIER: &str = "test-device-verifier";
    /// Precomputed S256 challenge for [`PKCE_VERIFIER`].
    const PKCE_CHALLENGE: &str = "-h3fMaFx46QpbqSYNy5y8dFicxDubLWG6tjHbsu4rcw";
    /// Password used for fixture users (longer than the minimum policy length).
    const PASSWORD: &str = "correct-horse-battery";
    /// A generous TTL for fixtures that should not expire mid-test.
    const LONG_TTL: Duration = Duration::from_secs(3_600);

    /// Drive an async test body on a current-thread runtime.
    ///
    /// `ankh-db` deliberately depends on tokio without the `macros` feature, so tests use an
    /// explicit runtime rather than `#[tokio::test]`.
    fn block_on<F: Future>(future: F) -> F::Output {
        TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime")
            .block_on(future)
    }

    /// Seed hook that inserts nothing; tests create exactly the rows they need.
    async fn no_seed(_pool: AnkhDbPool) -> DbResult<()> {
        Ok(())
    }

    /// Add a user and return its ID.
    async fn add_user(db: &mut AnkhDb, username: &str, email: &str) -> DbResult<UserId> {
        db.add_user(username, email, PASSWORD).await?;
        Ok(db.get_user_by_email(email).await?.id)
    }

    #[test]
    fn user_signup_login_and_session_lifecycle() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            add_user(&mut db, "alice", "alice@example.com").await?;

            // Unverified by default, then verified after marking.
            let user = db.get_user_by_email("alice@example.com").await?;
            assert_eq!(user.username, "alice");
            assert!(user.verified_at.is_none());
            db.mark_email_verified("alice@example.com").await?;
            assert!(
                db.get_user_by_email("alice@example.com")
                    .await?
                    .verified_at
                    .is_some()
            );

            // Wrong password is rejected.
            assert!(matches!(
                db.signin("alice@example.com", "wrong-password", LONG_TTL)
                    .await,
                Err(Error::InvalidCredentials)
            ));

            // Sign in, observe the session, then delete it.
            let token = db.signin("alice@example.com", PASSWORD, LONG_TTL).await?;
            assert_eq!(db.get_session(&token).await?.email, "alice@example.com");
            db.touch_session(&token).await?;
            db.delete_session(&token).await?;
            assert!(matches!(
                db.get_session(&token).await,
                Err(Error::SessionMissing(_))
            ));
            Ok(())
        }))
    }

    #[test]
    fn org_create_enforces_single_owner() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            let alice = add_user(&mut db, "alice", "alice@example.com").await?;
            let bob = add_user(&mut db, "bob", "bob@example.com").await?;

            let org = db.create_org("team-one", Some("Team One"), alice).await?;
            assert_eq!(db.get_org_owner(org).await?.user_id, alice);
            assert_eq!(db.list_orgs_for_user(alice).await?.len(), 1);

            // A second owner violates the one-owner-per-org constraint.
            assert!(
                db.add_org_member(org, bob, OrgRole::Owner, Some(alice))
                    .await
                    .is_err(),
                "schema must reject a second owner"
            );
            Ok(())
        }))
    }

    #[test]
    fn org_membership_role_change_and_ownership_transfer() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            let alice = add_user(&mut db, "alice", "alice@example.com").await?;
            let bob = add_user(&mut db, "bob", "bob@example.com").await?;
            let org = db.create_org("team-one", None, alice).await?;

            db.add_org_member(org, bob, OrgRole::Member, Some(alice))
                .await?;
            assert_eq!(db.get_org_member(org, bob).await?.role, OrgRole::Member);

            db.set_org_member_role(org, bob, OrgRole::Admin).await?;
            assert_eq!(db.get_org_member(org, bob).await?.role, OrgRole::Admin);

            db.transfer_org_ownership(org, bob).await?;
            assert_eq!(db.get_org_owner(org).await?.user_id, bob);
            assert_ne!(db.get_org_member(org, alice).await?.role, OrgRole::Owner);
            Ok(())
        }))
    }

    #[test]
    fn device_auth_grant_consume_is_single_use_and_verifier_checked() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            let alice = add_user(&mut db, "alice", "alice@example.com").await?;

            let created = db
                .create_device_auth_grant(DeviceAuthGrantRequest {
                    user_id: alice,
                    code_challenge: PKCE_CHALLENGE,
                    state: "state-token",
                    redirect_port: 49_152,
                    device_name: "Demo CLI",
                    platform: DevicePlatform::Macos,
                    ttl: LONG_TTL,
                })
                .await?;

            // Wrong verifier is rejected.
            assert!(
                db.consume_device_auth_grant(&created.code, "not-the-verifier")
                    .await
                    .is_err()
            );

            // Correct verifier consumes it once.
            let grant = db
                .consume_device_auth_grant(&created.code, PKCE_VERIFIER)
                .await?;
            assert_eq!(grant.user_id, alice);

            // A second consume fails (already consumed).
            assert!(
                db.consume_device_auth_grant(&created.code, PKCE_VERIFIER)
                    .await
                    .is_err()
            );
            Ok(())
        }))
    }

    #[test]
    fn device_session_validate_and_revoke() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            let alice = add_user(&mut db, "alice", "alice@example.com").await?;

            let created = db
                .create_device_session(alice, "Demo CLI", &DevicePlatform::Linux, LONG_TTL)
                .await?;
            assert_eq!(
                db.validate_device_session(&created.token).await?.id,
                created.session.id
            );
            assert_eq!(db.list_device_sessions_for_user(alice).await?.len(), 1);

            db.revoke_device_session(created.session.id, alice).await?;
            assert!(
                db.validate_device_session(&created.token).await.is_err(),
                "revoked device session must not validate"
            );
            Ok(())
        }))
    }

    #[test]
    fn token_kinds_are_single_use_and_kind_scoped() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            add_user(&mut db, "alice", "alice@example.com").await?;

            let token = db
                .create_token("alice@example.com", TokenKind::EmailVerification, LONG_TTL)
                .await?;
            // Peeking does not consume; consuming with the wrong kind fails.
            assert_eq!(
                db.peek_token(&token, TokenKind::EmailVerification).await?,
                "alice@example.com"
            );
            assert!(
                db.consume_token(&token, TokenKind::PasswordReset)
                    .await
                    .is_err()
            );

            // Consuming with the right kind works once.
            assert_eq!(
                db.consume_token(&token, TokenKind::EmailVerification)
                    .await?,
                "alice@example.com"
            );
            assert!(
                db.consume_token(&token, TokenKind::EmailVerification)
                    .await
                    .is_err()
            );
            Ok(())
        }))
    }

    #[test]
    fn namespace_suspend_and_reinstate_bumps_generation() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            let alice = add_user(&mut db, "alice", "alice@example.com").await?;
            let org = db.create_org("team-one", None, alice).await?;
            let detail = db.get_org_by_id(org).await?;

            let suspended = db
                .set_namespace_suspended(detail.namespace_id, true)
                .await?;
            assert!(suspended.suspended);
            assert!(
                suspended.r#gen > detail.namespace_gen,
                "suspending must bump the generation"
            );

            let reinstated = db
                .set_namespace_suspended(detail.namespace_id, false)
                .await?;
            assert!(!reinstated.suspended);
            assert!(
                reinstated.r#gen > suspended.r#gen,
                "reinstating must bump the generation again"
            );
            Ok(())
        }))
    }

    #[test]
    fn list_users_paginates_with_cursor_round_trip() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            let mut expected = HashSet::new();
            for n in 0..5 {
                let email = format!("user-{n}@example.com");
                add_user(&mut db, &format!("user-{n}"), &email).await?;
                let _inserted = expected.insert(email);
            }

            // Walk every page of size 2 and collect the emails seen.
            let mut seen = HashSet::new();
            let mut cursor: Option<String> = None;
            let mut pages = 0;
            loop {
                let (page, next) = db.list_users(2, cursor.as_deref(), None).await?;
                assert!(page.len() <= 2);
                for user in page {
                    assert!(seen.insert(user.email), "pages must not overlap");
                }
                pages += 1;
                match next {
                    Some(token) => cursor = Some(token),
                    None => break,
                }
                assert!(pages <= 5, "pagination did not terminate");
            }
            assert_eq!(seen, expected);
            Ok(())
        }))
    }

    #[test]
    fn list_users_stops_when_count_is_an_exact_multiple_of_the_page_size() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let mut db = fresh.get().await?;
            for n in 0..4 {
                add_user(
                    &mut db,
                    &format!("user-{n}"),
                    &format!("user-{n}@example.com"),
                )
                .await?;
            }

            // Four users at page size two is exactly two full pages. The second
            // page must report no further cursor rather than a cursor pointing
            // at an empty third page.
            let (first, first_cursor) = db.list_users(2, None, None).await?;
            assert_eq!(first.len(), 2);
            let cursor = first_cursor.expect("first page has a successor");

            let (second, second_cursor) = db.list_users(2, Some(cursor.as_str()), None).await?;
            assert_eq!(second.len(), 2);
            assert!(
                second_cursor.is_none(),
                "a full final page must not advertise an empty next page"
            );
            Ok(())
        }))
    }

    #[test]
    fn schema_apply_and_initialize_are_idempotent() -> DbResult<()> {
        block_on(with_fresh_db(no_seed, |fresh| async move {
            let db = fresh.get().await?;
            // The fresh DB is already bootstrapped; re-applying must be safe.
            db.apply_schema().await?;
            db.initialize().await?;
            db.initialize().await?;
            assert_eq!(db.version().await?, Some(ANKH_SCHEMA_VERSION));
            Ok(())
        }))
    }
}
