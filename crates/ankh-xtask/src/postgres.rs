//! Local Postgres lifecycle helpers.

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use crate::command::{XtaskResult, require_bins, run_status};

/// Error returned when a non-workspace Postgres already occupies the port.
fn foreign_port_error(config: &PostgresConfig<'_>) -> Box<dyn Error> {
    format!(
        "Postgres is already listening on port {}, but it is not the workspace instance at {}; stop the foreign server first",
        config.port,
        config.paths.data_dir.display()
    )
    .into()
}

/// Binaries required for local Postgres orchestration.
pub const REQUIRED_BINS: &[&str] = &[
    "initdb",
    "postgres",
    "pg_isready",
    "createdb",
    "psql",
    "pg_ctl",
];

/// Hostname used for local Postgres connections.
pub const LOCALHOST: &str = "localhost";
/// Maintenance database used for readiness checks.
pub const POSTGRES_DATABASE: &str = "postgres";
/// Number of readiness polls performed before giving up on Postgres startup.
pub const DEFAULT_READY_ATTEMPTS: u32 = 80;
/// Delay between readiness polls.
pub const DEFAULT_READY_BACKOFF: Duration = Duration::from_millis(100);

/// Paths owned by a workspace-local Postgres instance.
#[derive(Debug, Clone)]
pub struct PostgresPaths {
    /// Data directory.
    pub data_dir: PathBuf,
    /// Unix socket directory.
    pub socket_dir: PathBuf,
    /// Log file.
    pub log_file: PathBuf,
}

impl PostgresPaths {
    /// Build paths from a workspace root and relative path strings.
    #[must_use]
    pub fn from_root(
        workspace_root: &Path,
        data_dir: &str,
        socket_dir: &str,
        log_file: &str,
    ) -> Self {
        Self {
            data_dir: workspace_root.join(data_dir),
            socket_dir: workspace_root.join(socket_dir),
            log_file: workspace_root.join(log_file),
        }
    }
}

/// Shared Postgres lifecycle configuration.
#[derive(Debug, Clone)]
pub struct PostgresConfig<'a> {
    /// Hostname used for local connections.
    pub host: &'a str,
    /// Port to bind and inspect.
    pub port: u16,
    /// Maintenance database used by `pg_isready`.
    pub maintenance_database: &'a str,
    /// Workspace-local Postgres paths.
    pub paths: PostgresPaths,
    /// Databases created on a fresh data directory.
    pub bootstrap_databases: &'a [&'a str],
    /// Database seeded by `run_db` when seeding is requested.
    pub dev_database: &'a str,
    /// SQL used by `run_reset_db` to clear a database before bootstrapping.
    pub drop_tables_sql: &'a str,
}

impl<'a> PostgresConfig<'a> {
    /// Create a configuration with conventional localhost readiness settings.
    #[must_use]
    pub fn local(
        port: u16,
        paths: PostgresPaths,
        bootstrap_databases: &'a [&'a str],
        dev_database: &'a str,
        drop_tables_sql: &'a str,
    ) -> Self {
        Self {
            host: LOCALHOST,
            port,
            maintenance_database: POSTGRES_DATABASE,
            paths,
            bootstrap_databases,
            dev_database,
            drop_tables_sql,
        }
    }
}

/// User request for a local Postgres command.
#[derive(Debug, Clone, Default)]
pub struct DbRequest {
    /// Recreate the data directory from scratch.
    pub recreate: bool,
    /// Only report status without starting or stopping.
    pub status: bool,
    /// Stop the instance if it is running.
    pub stop: bool,
}

/// Live status for the workspace Postgres instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostgresStatus {
    /// Port checked for readiness.
    pub port: u16,
    /// True when the workspace data directory reports a running server.
    pub running: bool,
    /// True when `pg_isready` succeeds on the configured port.
    pub healthy: bool,
    /// True when another server occupies the port while this data dir is stopped.
    pub conflict: bool,
}

impl PostgresStatus {
    /// Render the status label used by dev status commands.
    #[must_use]
    pub fn label(self) -> &'static str {
        if self.conflict {
            "conflict"
        } else if self.running && self.healthy {
            "running"
        } else if self.running {
            "unhealthy"
        } else {
            "stopped"
        }
    }
}

/// Controls whether startup reseeds databases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedMode {
    /// Always reseed after startup.
    Always,
    /// Only reseed when bootstrapping a fresh data directory.
    FreshOnly,
}

impl SeedMode {
    /// Return true when the selected mode should reseed after startup.
    #[must_use]
    pub fn should_seed(self, fresh: bool) -> bool {
        matches!(self, Self::Always) || fresh
    }
}

/// Start, stop, recreate, or report a workspace-local Postgres instance.
pub fn run_db<Bootstrap, Seed>(
    config: &PostgresConfig<'_>,
    request: &DbRequest,
    seed_mode: SeedMode,
    bootstrap: &Bootstrap,
    seed: &Seed,
) -> XtaskResult
where
    Bootstrap: Fn(u16, &str) -> XtaskResult,
    Seed: Fn(u16, &str) -> XtaskResult,
{
    require_bins(REQUIRED_BINS)?;

    if request.status {
        print_status(&config.paths.data_dir)?;
        return Ok(());
    }

    if request.stop {
        print_stop_result(
            &config.paths.data_dir,
            stop_if_running_at_path(&config.paths.data_dir)?,
        );
        return Ok(());
    }

    ensure_dirs(&config.paths)?;

    if request.recreate && config.paths.data_dir.exists() {
        if postgres_running(&config.paths.data_dir)? {
            println!("Stopping running Postgres for --recreate...");
            stop_postgres(&config.paths.data_dir)?;
        }
        fs::remove_dir_all(&config.paths.data_dir)?;
    }

    if port_ready(config)? && !postgres_running(&config.paths.data_dir)? {
        return Err(foreign_port_error(config));
    }

    let fresh = !config.paths.data_dir.exists();
    let mut restarted = false;
    if fresh {
        init_db(&config.paths.data_dir)?;
    } else if postgres_running(&config.paths.data_dir)? {
        println!("Postgres already running; restarting with workspace settings...");
        stop_postgres(&config.paths.data_dir)?;
        restarted = true;
    }

    start_postgres(config)?;
    if let Err(err) = wait_ready(config) {
        stop_postgres(&config.paths.data_dir)?;
        return Err(err);
    }

    if fresh && bootstrap_databases(config, bootstrap).is_err() {
        stop_postgres(&config.paths.data_dir)?;
        return Err("failed to bootstrap databases".into());
    }

    if seed_mode.should_seed(fresh)
        && let Err(err) = seed(config.port, config.dev_database)
    {
        stop_postgres(&config.paths.data_dir)?;
        return Err(err);
    }

    let status_label = if fresh {
        "initialized"
    } else if restarted {
        "restarted"
    } else {
        "started"
    };
    println!(
        "Postgres {status_label} on port {}. Logs: {}",
        config.port,
        config.paths.log_file.display()
    );
    Ok(())
}

/// Ensure the workspace Postgres instance is running without reseeding.
pub fn ensure_db<Bootstrap, Seed>(
    config: &PostgresConfig<'_>,
    bootstrap: &Bootstrap,
    seed: &Seed,
) -> XtaskResult
where
    Bootstrap: Fn(u16, &str) -> XtaskResult,
    Seed: Fn(u16, &str) -> XtaskResult,
{
    require_bins(REQUIRED_BINS)?;

    if config.paths.data_dir.exists() && postgres_running(&config.paths.data_dir)? {
        return Ok(());
    }

    if port_ready(config)? {
        return Err(foreign_port_error(config));
    }

    run_db(
        config,
        &DbRequest::default(),
        SeedMode::FreshOnly,
        bootstrap,
        seed,
    )
}

/// Report live status for the workspace Postgres instance.
pub fn postgres_status(config: &PostgresConfig<'_>) -> XtaskResult<PostgresStatus> {
    let running = postgres_running(&config.paths.data_dir)?;
    let healthy = port_ready(config)?;
    Ok(PostgresStatus {
        port: config.port,
        running,
        healthy,
        conflict: healthy && !running,
    })
}

/// Stop the workspace Postgres instance if it is running.
pub fn stop_if_running(config: &PostgresConfig<'_>) -> XtaskResult<bool> {
    stop_if_running_at_path(&config.paths.data_dir)
}

/// Open an interactive psql shell to a database.
pub fn run_psql(config: &PostgresConfig<'_>, database: &str) -> XtaskResult {
    require_bins(&["psql"])?;
    run_status(
        Command::new("psql").args([
            "-h",
            config.host,
            "-p",
            &config.port.to_string(),
            "-d",
            database,
        ]),
        "psql",
    )
}

/// Reset a database by dropping tables, bootstrapping schemas, and seeding fixtures.
pub fn run_reset_db<Bootstrap, Seed>(
    config: &PostgresConfig<'_>,
    database: &str,
    bootstrap: &Bootstrap,
    seed: &Seed,
) -> XtaskResult
where
    Bootstrap: Fn(u16, &str) -> XtaskResult,
    Seed: Fn(u16, &str) -> XtaskResult,
{
    require_bins(&["psql"])?;
    let port = config.port.to_string();
    run_status(
        Command::new("psql").args([
            "-h",
            config.host,
            "-p",
            &port,
            "-d",
            database,
            "-c",
            config.drop_tables_sql,
        ]),
        "drop tables",
    )?;

    bootstrap(config.port, database)?;
    seed(config.port, database)
}

/// Create parent directories needed for data, socket, and log paths.
fn ensure_dirs(paths: &PostgresPaths) -> XtaskResult {
    if let Some(parent) = paths.data_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(&paths.socket_dir)?;
    if let Some(parent) = paths.log_file.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Print current Postgres status for the workspace data directory.
fn print_status(data_dir: &Path) -> XtaskResult {
    let output = Command::new("pg_ctl")
        .args(["status", "-D", path_text(data_dir).as_str()])
        .output()?;

    if output.status.success() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    } else {
        println!("postgres not running (data dir: {})", data_dir.display());
    }
    Ok(())
}

/// Print the result of a stop request.
fn print_stop_result(data_dir: &Path, stopped: bool) {
    if stopped {
        println!("postgres stopped (data dir: {})", data_dir.display());
    } else if data_dir.exists() {
        println!("postgres not running (data dir: {})", data_dir.display());
    } else {
        println!(
            "postgres not initialized (data dir: {})",
            data_dir.display()
        );
    }
}

/// Initialize a Postgres data directory.
fn init_db(data_dir: &Path) -> XtaskResult {
    run_status(
        Command::new("initdb").args([
            "-D",
            path_text(data_dir).as_str(),
            "-A",
            "trust",
            "--encoding",
            "UTF8",
        ]),
        "initdb",
    )
}

/// Return true if Postgres is already running for the provided data directory.
fn postgres_running(data_dir: &Path) -> XtaskResult<bool> {
    let status = Command::new("pg_ctl")
        .args(["-D", path_text(data_dir).as_str(), "status"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success())
}

/// Start a daemonized Postgres server with workspace-local sockets and logging.
fn start_postgres(config: &PostgresConfig<'_>) -> XtaskResult {
    let socket = path_text(&config.paths.socket_dir);
    let options = format!(
        "-p {} -k {socket} -c listen_addresses={} -c unix_socket_directories={socket}",
        config.port, config.host
    );
    run_status(
        Command::new("pg_ctl").args([
            "start",
            "-w",
            "-D",
            path_text(&config.paths.data_dir).as_str(),
            "-l",
            path_text(&config.paths.log_file).as_str(),
            "-o",
            &options,
        ]),
        "pg_ctl start",
    )
}

/// Poll until Postgres reports readiness.
fn wait_ready(config: &PostgresConfig<'_>) -> XtaskResult {
    for _ in 0..DEFAULT_READY_ATTEMPTS {
        if port_ready(config)? {
            return Ok(());
        }
        thread::sleep(DEFAULT_READY_BACKOFF);
    }
    Err("postgres did not become ready in time".into())
}

/// Return true when `pg_isready` succeeds on the configured port.
fn port_ready(config: &PostgresConfig<'_>) -> XtaskResult<bool> {
    let status = Command::new("pg_isready")
        .args([
            "-h",
            config.host,
            "-p",
            &config.port.to_string(),
            "-d",
            config.maintenance_database,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success())
}

/// Create databases and bootstrap schemas.
fn bootstrap_databases<Bootstrap>(config: &PostgresConfig<'_>, bootstrap: &Bootstrap) -> XtaskResult
where
    Bootstrap: Fn(u16, &str) -> XtaskResult,
{
    let port = config.port.to_string();
    for database in config.bootstrap_databases {
        run_status(
            Command::new("createdb").args(["-h", config.host, "-p", &port, database]),
            "createdb",
        )?;
        bootstrap(config.port, database)?;
    }
    Ok(())
}

/// Stop the running Postgres process if it is still alive.
fn stop_postgres(data_dir: &Path) -> XtaskResult {
    run_status(
        Command::new("pg_ctl").args([
            "stop",
            "-w",
            "-m",
            "fast",
            "-D",
            path_text(data_dir).as_str(),
        ]),
        "pg_ctl stop",
    )
}

/// Stop Postgres if it is running.
fn stop_if_running_at_path(data_dir: &Path) -> XtaskResult<bool> {
    if data_dir.exists() && postgres_running(data_dir)? {
        stop_postgres(data_dir)?;
        return Ok(true);
    }
    Ok(false)
}

/// Convert a path to an owned string suitable for command arguments.
fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{PostgresConfig, PostgresPaths, PostgresStatus, SeedMode};

    #[test]
    fn builds_workspace_local_paths() {
        let root = Path::new("/tmp/project");
        let paths = PostgresPaths::from_root(root, "tmp/pgdata", "tmp/run", "tmp/logs/pg.log");
        assert_eq!(paths.data_dir, root.join("tmp/pgdata"));
        assert_eq!(paths.socket_dir, root.join("tmp/run"));
        assert_eq!(paths.log_file, root.join("tmp/logs/pg.log"));
    }

    #[test]
    fn local_config_uses_standard_readiness_settings() {
        let paths = PostgresPaths::from_root(Path::new("/tmp/project"), "data", "run", "log");
        let databases = ["app-test", "app-dev"];
        let config = PostgresConfig::local(55432, paths, &databases, "app-dev", "DROP TABLE x");
        assert_eq!(config.host, "localhost");
        assert_eq!(config.maintenance_database, "postgres");
        assert_eq!(config.bootstrap_databases, &databases);
    }

    #[test]
    fn seed_mode_matches_freshness() {
        assert!(SeedMode::Always.should_seed(false));
        assert!(SeedMode::FreshOnly.should_seed(true));
        assert!(!SeedMode::FreshOnly.should_seed(false));
    }

    #[test]
    fn status_labels_include_conflicts() {
        assert_eq!(
            PostgresStatus {
                port: 1,
                running: false,
                healthy: true,
                conflict: true,
            }
            .label(),
            "conflict"
        );
    }
}
