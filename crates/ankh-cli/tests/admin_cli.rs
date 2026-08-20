//! Integration tests for the shared Ankh admin CLI plumbing.

#[cfg(test)]
mod tests {
    //! DB-backed CLI tests using the in-process Ankh admin router.

    use std::{
        fs,
        path::{Path, PathBuf},
        process,
        sync::atomic::{AtomicU64, Ordering},
    };

    use ankh_cli::{
        AdminClient, AuthCommand, CommonCommand, CommonRuntime, Config, DeviceSessionsCommand,
        Error, Format, GlobalArgs, ListArgs, ListDeviceSessionsParams, ListOrgsParams,
        ListSessionsParams, ListSysadminsParams, ListUsersParams, OrgMembersCommand, OrgsCommand,
        ProductInfo, Render, SessionsCommand, SettingsCommand, SysadminsCommand, UsersCommand,
        WaitlistCommand, run_common,
    };
    use ankh_constants::DEVICE_SESSION_TTL;
    use ankh_db::{
        AnkhDbPool,
        test_support::{FreshDb, with_fresh_db},
    };
    use ankh_mail::{MailBranding, MailCatalog, PublicBaseUrl, RecordingMailer};
    use ankh_testdata::{ADMIN, ALICE, BOB, seed_identities};
    use ankh_types::{DevicePlatform, admin::WhoamiResponse};
    use ankh_web::{
        AnkhWebConfig, AnkhWebState, CookieConfig, MailState, test_support::TestAppHarness,
    };
    use axum::{Router, routing::get};
    use clap::Parser;
    use reqwest::Method;
    use tokio::{net::TcpListener, task::JoinHandle};

    /// Product metadata used by the shared CLI tests.
    const PRODUCT: ProductInfo = ProductInfo::new("ankh-test-cli", ".ankh-test.toml");
    /// Profile name used by the shared CLI tests.
    const PROFILE: &str = "dev";
    /// Monotonic suffix for test-owned temporary directories.
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    /// Parser harness that exposes the common globals and commands directly.
    #[derive(Debug, Parser)]
    struct HarnessCli {
        /// Shared globals under test.
        #[command(flatten)]
        global: GlobalArgs,
        /// Shared command group under test.
        #[command(subcommand)]
        command: CommonCommand,
    }

    /// Owned temporary directory rooted under the repository `tmp/` directory.
    #[derive(Debug)]
    struct TestTempDir {
        /// Path removed when the helper is dropped.
        path: PathBuf,
    }

    impl TestTempDir {
        /// Create a fresh test-owned temporary directory.
        fn new(label: &str) -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = workspace_root()
                .join("tmp")
                .join("ankh-cli-tests")
                .join(format!("{}-{label}-{id}", process::id()));
            fs::create_dir_all(&path).expect("create test temp directory");
            Self { path }
        }

        /// Return the temporary directory path.
        fn path(&self) -> &Path {
            self.path.as_path()
        }
    }

    impl Drop for TestTempDir {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.path)
                && self.path.exists()
            {
                eprintln!(
                    "failed to remove test temp directory {}: {error}",
                    self.path.display()
                );
            }
        }
    }

    /// Loopback HTTP server for a router under test.
    struct TestServer {
        /// Base URL for the server.
        base_url: String,
        /// Background server task.
        task: JoinHandle<()>,
    }

    impl TestServer {
        /// Start a server backed by a seeded Ankh pool.
        async fn spawn(pool: AnkhDbPool) -> Self {
            let state = test_state(pool);
            Self::spawn_router(TestAppHarness::new(state).router()).await
        }

        /// Start a server for an arbitrary Axum router.
        async fn spawn_router(router: Router) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind test HTTP listener");
            let address = listener.local_addr().expect("read listener address");
            let task = tokio::spawn(async move {
                if let Err(error) = axum::serve(listener, router).await {
                    eprintln!("test HTTP server failed: {error}");
                }
            });
            Self {
                base_url: format!("http://{address}"),
                task,
            }
        }

        /// Return the server base URL.
        fn base_url(&self) -> &str {
            self.base_url.as_str()
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    /// Return the repository root for test-owned artifacts.
    fn workspace_root() -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        root.canonicalize().unwrap_or(root)
    }

    /// Build a web state object for seeded router tests.
    fn test_state(pool: AnkhDbPool) -> AnkhWebState {
        let branding = MailBranding::new(
            "Ankh",
            PublicBaseUrl::new("http://127.0.0.1:52700").expect("valid public base url"),
            "no-reply@example.com",
            "support@example.com",
        );
        let mail = MailState::new(RecordingMailer::new(), MailCatalog::shared(), branding);
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

    /// Build shared global arguments for one command invocation.
    fn global_args(config_path: &Path, base_url: Option<&str>, format: Format) -> GlobalArgs {
        GlobalArgs {
            base_url: base_url.map(str::to_owned),
            format,
            config_path: Some(config_path.to_path_buf()),
            profile: Some(PROFILE.to_owned()),
            quiet: true,
            verbose: false,
            trace_id: None,
        }
    }

    /// Build a common runtime for one command invocation.
    fn common_runtime(config_path: &Path, base_url: Option<&str>, format: Format) -> CommonRuntime {
        CommonRuntime::new(PRODUCT, global_args(config_path, base_url, format))
    }

    /// Load the saved bearer token from a CLI config file.
    fn saved_token(config_path: &Path) -> String {
        let config = Config::load_from_path(config_path).expect("load CLI config");
        let (_profile_name, profile) = config.get_profile(Some(PROFILE)).expect("load profile");
        profile.token.clone().expect("profile stores token")
    }

    /// Run the common login command and return the persisted token.
    async fn login(config_path: &Path, base_url: &str) -> String {
        run_common(
            common_runtime(config_path, None, Format::Table),
            CommonCommand::Auth {
                command: AuthCommand::Login {
                    base_url: Some(base_url.to_owned()),
                    email: Some(ADMIN.email.to_owned()),
                    password: Some(ADMIN.password.to_owned()),
                },
            },
        )
        .await
        .expect("login succeeds");
        saved_token(config_path)
    }

    /// Run a shared common command against a saved profile.
    async fn run_saved(config_path: &Path, command: CommonCommand) {
        run_common(common_runtime(config_path, None, Format::Json), command)
            .await
            .expect("common command succeeds");
    }

    /// Create a client authenticated with the persisted profile token.
    fn saved_client(config_path: &Path, base_url: &str) -> AdminClient {
        AdminClient::new(base_url).with_token(saved_token(config_path))
    }

    /// Proves shared globals use the standardized flag spelling.
    #[test]
    fn shared_globals_use_standardized_flag_names() {
        let parsed = HarnessCli::try_parse_from([
            "harness",
            "--config",
            "tmp/cli.toml",
            "--profile",
            "dev",
            "--base-url",
            "http://127.0.0.1:9000",
            "--format",
            "json",
            "--quiet",
            "--verbose",
            "--trace-id",
            "trace-123",
            "auth",
            "whoami",
        ])
        .expect("parse standardized globals");

        assert_eq!(
            parsed.global.config_path.as_deref(),
            Some(Path::new("tmp/cli.toml"))
        );
        assert_eq!(
            parsed.global.base_url.as_deref(),
            Some("http://127.0.0.1:9000")
        );
        assert_eq!(parsed.global.format, Format::Json);
        assert!(parsed.global.quiet);
        assert!(parsed.global.verbose);
        assert_eq!(parsed.global.trace_id.as_deref(), Some("trace-123"));
        assert!(matches!(
            parsed.command,
            CommonCommand::Auth {
                command: AuthCommand::Whoami
            }
        ));

        let old_flag = HarnessCli::try_parse_from([
            "harness",
            "--config-path",
            "tmp/cli.toml",
            "auth",
            "whoami",
        ]);
        assert!(old_flag.is_err());
    }

    /// Proves common commands use persisted auth and mutate through real routes.
    #[tokio::test(flavor = "current_thread")]
    async fn common_commands_use_saved_profile_and_real_admin_routes() -> ankh_db::Result<()> {
        with_fresh_db(seed_identities, |fresh| async move {
            let server = TestServer::spawn(fresh.pool().clone()).await;
            let temp = TestTempDir::new("commands");
            let config_path = temp.path().join("config.toml");
            let token = login(&config_path, server.base_url()).await;
            let client = AdminClient::new(server.base_url()).with_token(token);

            run_saved(
                &config_path,
                CommonCommand::Auth {
                    command: AuthCommand::Whoami,
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::Users {
                    command: UsersCommand::List {
                        list: ListArgs {
                            limit: 10,
                            cursor: None,
                        },
                        email: Some(ALICE.email.to_owned()),
                    },
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::Sysadmins {
                    command: SysadminsCommand::List {
                        list: ListArgs {
                            limit: 10,
                            cursor: None,
                        },
                    },
                },
            )
            .await;

            let alice_sessions = client
                .list_sessions(&ListSessionsParams {
                    limit: Some(10),
                    ..ListSessionsParams::default()
                })
                .await
                .expect("list seeded sessions");
            let session_id = alice_sessions
                .sessions
                .first()
                .expect("seeded session exists")
                .id
                .clone();
            run_saved(
                &config_path,
                CommonCommand::Sessions {
                    command: SessionsCommand::List {
                        list: ListArgs {
                            limit: 10,
                            cursor: None,
                        },
                        user_id: None,
                        status: None,
                    },
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::Sessions {
                    command: SessionsCommand::Revoke { id: session_id },
                },
            )
            .await;

            let device_id = seed_device_session(&fresh).await?;
            run_saved(
                &config_path,
                CommonCommand::DeviceSessions {
                    command: DeviceSessionsCommand::List {
                        list: ListArgs {
                            limit: 10,
                            cursor: None,
                        },
                        user_id: None,
                        status: None,
                    },
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::DeviceSessions {
                    command: DeviceSessionsCommand::Revoke { id: device_id },
                },
            )
            .await;

            let orgs = client
                .list_orgs(&ListOrgsParams {
                    limit: Some(10),
                    ..ListOrgsParams::default()
                })
                .await
                .expect("list seeded organizations");
            let org_id = orgs.orgs.first().expect("seeded org exists").id.clone();
            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::List {
                        list: ListArgs {
                            limit: 10,
                            cursor: None,
                        },
                    },
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::Invites {
                        command: ankh_cli::OrgInvitesCommand::Create {
                            org_id,
                            email: "new-invite@example.com".to_owned(),
                        },
                    },
                },
            )
            .await;

            let mut db = fresh.get().await?;
            db.add_user("charlie", "charlie@example.com", "charlie-pass")
                .await?;
            let charlie = db.get_user_by_email("charlie@example.com").await?;
            drop(db);
            run_saved(
                &config_path,
                CommonCommand::Users {
                    command: UsersCommand::Remove {
                        id: charlie.id.to_string(),
                        yes: true,
                    },
                },
            )
            .await;

            Ok(())
        })
        .await
    }

    /// Exercises settings/waitlist, user invite/release, and the full org lifecycle via the CLI.
    #[tokio::test(flavor = "current_thread")]
    async fn cli_covers_settings_waitlist_users_and_org_lifecycle() -> ankh_db::Result<()> {
        with_fresh_db(seed_identities, |fresh| async move {
            let server = TestServer::spawn(fresh.pool().clone()).await;
            let temp = TestTempDir::new("lifecycle");
            let config_path = temp.path().join("config.toml");
            let _token = login(&config_path, server.base_url()).await;

            // Resolve seeded user IDs used as command arguments.
            let (alice_id, bob_id) = {
                let db = fresh.get().await?;
                (
                    db.get_user_by_email(ALICE.email).await?.id.to_string(),
                    db.get_user_by_email(BOB.email).await?.id.to_string(),
                )
            };

            // Settings: waitlist enable / status / disable.
            for command in [
                WaitlistCommand::Enable,
                WaitlistCommand::Status,
                WaitlistCommand::Disable,
            ] {
                run_saved(
                    &config_path,
                    CommonCommand::Settings {
                        command: SettingsCommand::Waitlist { command },
                    },
                )
                .await;
            }

            // Users: invite (waitlist bypass) and release of a waitlisted account.
            run_saved(
                &config_path,
                CommonCommand::Users {
                    command: UsersCommand::Invite {
                        email: "invitee@example.com".to_owned(),
                    },
                },
            )
            .await;
            {
                let mut db = fresh.get().await?;
                db.add_user("waiter", "waiter@example.com", "waiter-pass")
                    .await?;
                db.set_user_waitlisted("waiter@example.com", true).await?;
            }
            run_saved(
                &config_path,
                CommonCommand::Users {
                    command: UsersCommand::Release {
                        target: "waiter@example.com".to_owned(),
                    },
                },
            )
            .await;

            // Orgs: create, update, add member, set role, transfer, remove member, delete.
            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::Create {
                        name: "cli-org".to_owned(),
                        display_name: Some("CLI Org".to_owned()),
                        owner_id: alice_id.clone(),
                    },
                },
            )
            .await;
            let org_id = fresh
                .get()
                .await?
                .get_org_by_name("cli-org")
                .await?
                .id
                .to_string();

            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::Update {
                        id: org_id.clone(),
                        display_name: Some("CLI Org Renamed".to_owned()),
                    },
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::Members {
                        command: OrgMembersCommand::Add {
                            org_id: org_id.clone(),
                            user_id: bob_id.clone(),
                            role: "member".to_owned(),
                        },
                    },
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::Members {
                        command: OrgMembersCommand::SetRole {
                            org_id: org_id.clone(),
                            user_id: bob_id.clone(),
                            role: "admin".to_owned(),
                        },
                    },
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::Transfer {
                        id: org_id.clone(),
                        new_owner_id: bob_id.clone(),
                        yes: true,
                    },
                },
            )
            .await;
            // Alice is now a non-owner member and can be removed.
            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::Members {
                        command: OrgMembersCommand::Remove {
                            org_id: org_id.clone(),
                            user_id: alice_id.clone(),
                            yes: true,
                        },
                    },
                },
            )
            .await;
            run_saved(
                &config_path,
                CommonCommand::Orgs {
                    command: OrgsCommand::Remove {
                        id: org_id.clone(),
                        yes: true,
                    },
                },
            )
            .await;

            Ok(())
        })
        .await
    }

    /// Proves client query construction and both output formats against real responses.
    #[tokio::test(flavor = "current_thread")]
    async fn client_query_support_and_renderers_use_real_responses() -> ankh_db::Result<()> {
        with_fresh_db(seed_identities, |fresh| async move {
            let server = TestServer::spawn(fresh.pool().clone()).await;
            let temp = TestTempDir::new("render");
            let config_path = temp.path().join("config.toml");
            login(&config_path, server.base_url()).await;
            let client = saved_client(&config_path, server.base_url());

            let users = client
                .list_users(&ListUsersParams {
                    limit: Some(1),
                    email: Some(BOB.email.to_owned()),
                    ..ListUsersParams::default()
                })
                .await
                .expect("list users with query");
            assert_eq!(users.users.len(), 1);
            assert_eq!(users.users[0].email, BOB.email);

            let sysadmins = client
                .list_sysadmins(&ListSysadminsParams {
                    limit: Some(10),
                    ..ListSysadminsParams::default()
                })
                .await
                .expect("list sysadmins");
            let table = sysadmins.sysadmins.render_to_string(Format::Table);
            let json = sysadmins.sysadmins.render_to_string(Format::Json);
            assert!(table.contains("Email"));
            assert!(table.contains(ADMIN.email));
            assert!(json.contains(ADMIN.email));

            let devices = client
                .list_device_sessions(&ListDeviceSessionsParams {
                    limit: Some(10),
                    ..ListDeviceSessionsParams::default()
                })
                .await
                .expect("list device sessions");
            assert!(devices.sessions.is_empty());

            Ok(())
        })
        .await
    }

    /// Proves API error envelopes are converted into structured CLI errors.
    #[tokio::test(flavor = "current_thread")]
    async fn admin_error_response_becomes_structured_cli_error() -> ankh_db::Result<()> {
        with_fresh_db(seed_identities, |fresh| async move {
            let server = TestServer::spawn(fresh.pool().clone()).await;
            let error = AdminClient::new(server.base_url())
                .whoami()
                .await
                .expect_err("missing token is rejected");

            match error {
                Error::Api {
                    status,
                    code,
                    message,
                } => {
                    assert_eq!(status, 401);
                    assert_eq!(code, "unauthorized");
                    assert!(message.contains("authorization"));
                }
                other => panic!("expected API error, got {other:?}"),
            }

            Ok(())
        })
        .await
    }

    /// Proves malformed JSON responses are reported as invalid responses.
    #[tokio::test(flavor = "current_thread")]
    async fn malformed_response_becomes_invalid_response_error() {
        let server = TestServer::spawn_router(
            Router::new().route("/malformed", get(|| async { "this is not json" })),
        )
        .await;
        let client = AdminClient::new(server.base_url());
        let error = client
            .execute::<WhoamiResponse>(client.request(Method::GET, "/malformed"))
            .await
            .expect_err("malformed JSON is rejected");

        match error {
            Error::InvalidResponse(message) => {
                assert!(message.contains("expected ident"));
                assert!(message.contains("this is not json"));
            }
            other => panic!("expected invalid response, got {other:?}"),
        }
    }

    /// Seed one device session and return its ID.
    async fn seed_device_session(fresh: &FreshDb) -> ankh_db::Result<String> {
        let db = fresh.get().await?;
        let alice = db.get_user_by_email(ALICE.email).await?;
        let created = db
            .create_device_session(
                alice.id,
                "CLI Test Device",
                &DevicePlatform::Web,
                DEVICE_SESSION_TTL,
            )
            .await?;
        Ok(created.session.id.to_string())
    }
}
