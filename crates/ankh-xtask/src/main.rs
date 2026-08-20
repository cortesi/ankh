#![warn(missing_docs)]

//! Developer task runner for the Ankh workspace.

use std::{
    error::Error,
    path::{Path, PathBuf},
    process,
};

use ankh_db::{create_pg_pool_with_max_size, test_support::DEFAULT_POSTGRES_PORT};
use ankh_xtask::{
    command::{
        binary_available, exec_cargo, run_async_result, run_rustfmt, workspace_root_from_manifest,
    },
    frontend::{ensure_pnpm_dependencies, run_pnpm, run_pnpm_script_with_install},
    postgres::{self, DbRequest, PostgresConfig, PostgresPaths, SeedMode},
};
use clap::{Args, Parser, Subcommand};

/// Local Ankh test database name.
const TEST_DATABASE_NAME: &str = "ankh-test";
/// Relative path to local Postgres data.
const POSTGRES_DATA_RELATIVE_PATH: &str = "tmp/pgdata";
/// Relative path to local Postgres sockets.
const POSTGRES_SOCKET_RELATIVE_PATH: &str = "tmp/run";
/// Relative path to local Postgres logs.
const POSTGRES_LOG_RELATIVE_PATH: &str = "tmp/logs/postgres.log";
/// SQL used to reset the local Ankh test database.
const DROP_TABLES_SQL: &str = "DROP TABLE IF EXISTS org_invites, org_members, organizations, sysadmin_tokens, sysadmins, \
     invites, tokens, sessions, device_sessions, device_auth_grants, users, namespaces, \
     ankh_settings, ankh_schema_version CASCADE";

/// CLI entrypoint powered by `clap` derive parsing.
#[derive(Parser)]
#[command(
    name = "ankh-xtask",
    about = "Developer task runner for the Ankh workspace",
    disable_help_subcommand = true,
    arg_required_else_help = true
)]
struct Cli {
    /// Selected automation task.
    #[command(subcommand)]
    command: TaskCommand,
}

/// Supported automation tasks.
#[derive(Subcommand)]
enum TaskCommand {
    /// Manage the local Ankh Postgres instance used by DB integration tests.
    #[command(name = "db", subcommand)]
    Db(DbCommand),
    /// Run Rust and frontend tests.
    #[command(name = "test")]
    Test(PassthroughArgs),
    /// Run formatting and lint checks across the workspace.
    #[command(name = "tidy")]
    Tidy(PassthroughArgs),
    /// Run the leaf consumers' gates against this Ankh working tree.
    #[command(name = "check-siblings")]
    CheckSiblings,
    /// Run the local demo server (the full Ankh stack) against the dev Postgres.
    #[command(name = "demo")]
    Demo(DemoArgs),
}

/// Options for the local demo server.
#[derive(Args, Default)]
struct DemoArgs {
    /// HTTP port for the demo server.
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Seed deterministic demo identities before serving.
    #[arg(long)]
    seed: bool,
    /// Drop and recreate the database before serving.
    #[arg(long)]
    reset: bool,
    /// Skip building the demo frontend (serve a previously built bundle, or run Vite separately).
    #[arg(long)]
    no_frontend: bool,
}

/// Collector for passthrough arguments forwarded to cargo invocations.
#[derive(Args, Default)]
struct PassthroughArgs {
    /// Arguments forwarded to cargo invocations.
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "ARGS"
    )]
    passthrough: Vec<String>,
}

/// Local Postgres lifecycle commands.
#[derive(Subcommand)]
enum DbCommand {
    /// Start the local Postgres instance.
    #[command(name = "start")]
    Start(DbStartArgs),
    /// Stop the local Postgres instance.
    #[command(name = "stop")]
    Stop,
    /// Report whether the local Postgres instance is running.
    #[command(name = "status")]
    Status,
}

/// Options for starting the local Postgres instance.
#[derive(Args, Default)]
struct DbStartArgs {
    /// Port to expose Postgres on.
    #[arg(long, default_value_t = DEFAULT_POSTGRES_PORT)]
    port: u16,
    /// Drop and recreate the data directory before starting Postgres.
    #[arg(long)]
    recreate: bool,
}

/// Program entrypoint.
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

/// Parse CLI arguments and dispatch the selected task.
fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match cli.command {
        TaskCommand::Db(command) => run_db(&command),
        TaskCommand::Test(args) => run_test(&args.passthrough),
        TaskCommand::Tidy(args) => run_tidy(&args.passthrough),
        TaskCommand::CheckSiblings => run_check_siblings(),
        TaskCommand::Demo(args) => run_demo(&args),
    }
}

/// Ensure Postgres is ready (recreating it on `--reset`), then launch the demo server.
fn run_demo(args: &DemoArgs) -> Result<(), Box<dyn Error>> {
    if args.reset {
        let request = DbRequest {
            recreate: true,
            status: false,
            stop: false,
        };
        postgres::run_db(
            &postgres_config(DEFAULT_POSTGRES_PORT),
            &request,
            SeedMode::FreshOnly,
            &bootstrap_database,
            &seed_database,
        )?;
    } else {
        ensure_test_postgres()?;
    }

    if !args.no_frontend {
        build_demo_frontend()?;
    }

    let mut demo_args = vec!["--port".to_owned(), args.port.to_string()];
    if args.seed {
        demo_args.push("--seed".to_owned());
    }
    exec_cargo(
        &workspace_root(),
        &["run", "-p", "ankh-demo", "--"],
        &demo_args,
    )
}

/// Build the `@ankh/demo-web` SPA into the `ankh-demo` crate's `dist/` so the server can serve it.
fn build_demo_frontend() -> Result<(), Box<dyn Error>> {
    let frontend = frontend_root();
    ensure_pnpm_dependencies(&frontend)?;
    run_pnpm(&frontend, &["--filter", "@ankh/demo-web", "run", "build"])
}

/// Leaf consumer checkouts validated by `check-siblings`, relative to the Ankh root's parent.
const SIBLING_CONSUMERS: [&str; 2] = ["restless", "verber-web"];

/// Run each present leaf consumer's own `tidy` and `test` gates against this Ankh tree.
///
/// Both leaves consume Ankh through sibling path/`file:` dependencies, so an Ankh change can
/// break them. Each leaf's `cargo xtask tidy`/`test` is its full gate (Rust, generated
/// TypeScript freshness, and frontend), so this delegates rather than reimplementing them.
fn run_check_siblings() -> Result<(), Box<dyn Error>> {
    let Some(siblings_root) = workspace_root().parent().map(Path::to_path_buf) else {
        return Err("cannot resolve the parent directory of the Ankh workspace".into());
    };

    let mut checked = 0_usize;
    for consumer in SIBLING_CONSUMERS {
        let dir = siblings_root.join(consumer);
        if !dir.is_dir() {
            println!("-- skipping `{consumer}`: not found at {}", dir.display());
            continue;
        }
        println!("== checking sibling `{consumer}` at {}", dir.display());
        exec_cargo(&dir, &["xtask", "tidy"], &[])?;
        exec_cargo(&dir, &["xtask", "test"], &[])?;
        checked += 1;
    }

    if checked == 0 {
        println!("No sibling consumers found; nothing to check.");
    } else {
        println!("Checked {checked} sibling consumer(s) successfully.");
    }
    Ok(())
}

/// Run Rust and frontend tests against a guaranteed-reachable local Postgres.
///
/// DB-backed integration tests connect to the workspace Postgres, so this starts it first (or
/// fails with a clear message if a foreign server occupies the port). Rust tests run under
/// `cargo nextest`, the standard runner for this workspace; the frontend smoke tests run after.
fn run_test(passthrough: &[String]) -> Result<(), Box<dyn Error>> {
    ensure_test_postgres()?;

    if !binary_available("cargo-nextest") {
        return Err(
            "cargo-nextest is required for `cargo xtask test`; install it with \
             `cargo install cargo-nextest --locked`"
                .into(),
        );
    }
    exec_cargo(
        &workspace_root(),
        &["nextest", "run", "--all-features"],
        passthrough,
    )?;
    run_pnpm_script_with_install(&frontend_root(), "test")
}

/// Ensure the workspace Postgres used by DB integration tests is running.
fn ensure_test_postgres() -> Result<(), Box<dyn Error>> {
    postgres::ensure_db(
        &postgres_config(DEFAULT_POSTGRES_PORT),
        &bootstrap_database,
        &seed_database,
    )
}

/// Run formatting and lint checks.
fn run_tidy(passthrough: &[String]) -> Result<(), Box<dyn Error>> {
    let root = workspace_root();
    run_rustfmt(&root)?;
    check_generated_typescript()?;
    exec_cargo(
        &root,
        &[
            "clippy",
            "-q",
            "--all-targets",
            "--all-features",
            "--tests",
            "--examples",
            "--",
            "-D",
            "warnings",
        ],
        passthrough,
    )?;
    run_pnpm_script_with_install(&frontend_root(), "lint")?;
    run_pnpm_script_with_install(&frontend_root(), "format")
}

/// Check that generated TypeScript declarations are fresh.
fn check_generated_typescript() -> Result<(), Box<dyn Error>> {
    exec_cargo(
        &workspace_root(),
        &[
            "test",
            "-p",
            "ankh-types",
            "ts::tests::generated_typescript_declarations_are_current",
            "--",
            "--exact",
        ],
        &[],
    )
}

/// Manage the local Postgres instance for Ankh DB tests.
fn run_db(command: &DbCommand) -> Result<(), Box<dyn Error>> {
    let (port, request) = match command {
        DbCommand::Start(args) => (
            args.port,
            DbRequest {
                recreate: args.recreate,
                status: false,
                stop: false,
            },
        ),
        DbCommand::Stop => (
            DEFAULT_POSTGRES_PORT,
            DbRequest {
                recreate: false,
                status: false,
                stop: true,
            },
        ),
        DbCommand::Status => (
            DEFAULT_POSTGRES_PORT,
            DbRequest {
                recreate: false,
                status: true,
                stop: false,
            },
        ),
    };
    postgres::run_db(
        &postgres_config(port),
        &request,
        SeedMode::FreshOnly,
        &bootstrap_database,
        &seed_database,
    )
}

/// Bootstrap the Ankh schema in `database`.
fn bootstrap_database(port: u16, database: &str) -> Result<(), Box<dyn Error>> {
    let conn = format!("host=localhost port={port} dbname={database}");
    run_async_result(async move {
        let pool = create_pg_pool_with_max_size(conn, 1)?;
        let db = pool.get().await?;
        db.apply_schema().await?;
        db.initialize().await
    })
}

/// Seed hook for Ankh's local test database.
fn seed_database(_port: u16, database: &str) -> Result<(), Box<dyn Error>> {
    println!("Seeded {database} with Ankh schema metadata.");
    Ok(())
}

/// Build Postgres lifecycle configuration.
fn postgres_config(port: u16) -> PostgresConfig<'static> {
    static DATABASES: [&str; 1] = [TEST_DATABASE_NAME];
    PostgresConfig::local(
        port,
        PostgresPaths::from_root(
            &workspace_root(),
            POSTGRES_DATA_RELATIVE_PATH,
            POSTGRES_SOCKET_RELATIVE_PATH,
            POSTGRES_LOG_RELATIVE_PATH,
        ),
        &DATABASES,
        TEST_DATABASE_NAME,
        DROP_TABLES_SQL,
    )
}

/// Resolve the Ankh workspace root.
fn workspace_root() -> PathBuf {
    workspace_root_from_manifest(env!("CARGO_MANIFEST_DIR"))
}

/// Resolve the frontend workspace root.
fn frontend_root() -> PathBuf {
    workspace_root().join("frontend")
}
