//! Shared helpers for dev admin CLI auto-authentication.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::command::{XtaskResult, run_status};

/// Cargo-run target for a leaf admin CLI binary.
#[derive(Debug, Clone)]
pub struct CargoAdminCli {
    /// Workspace where `cargo run` should execute.
    pub workspace_root: PathBuf,
    /// Package name containing the leaf CLI binary.
    pub package: String,
}

impl CargoAdminCli {
    /// Build a new Cargo admin CLI target.
    #[must_use]
    pub fn new(workspace_root: impl Into<PathBuf>, package: impl Into<String>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            package: package.into(),
        }
    }
}

/// Dev admin login settings.
#[derive(Debug, Clone)]
pub struct AdminLogin {
    /// CLI config file that stores the dev profile credentials.
    pub config_path: PathBuf,
    /// CLI profile name.
    pub profile: String,
    /// Admin API base URL.
    pub base_url: String,
    /// Dev sysadmin email address.
    pub email: String,
    /// Dev sysadmin password.
    pub password: String,
}

impl AdminLogin {
    /// Build a new admin login settings value.
    #[must_use]
    pub fn new(
        config_path: impl Into<PathBuf>,
        profile: impl Into<String>,
        base_url: impl Into<String>,
        email: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            config_path: config_path.into(),
            profile: profile.into(),
            base_url: base_url.into(),
            email: email.into(),
            password: password.into(),
        }
    }
}

/// Authenticate the dev admin profile for a leaf CLI.
pub fn ensure_admin_login(cli: &CargoAdminCli, login: &AdminLogin) -> XtaskResult {
    run_cli(cli, &admin_login_args(login))
}

/// Run a leaf CLI package through Cargo with the supplied arguments.
pub fn run_cli(cli: &CargoAdminCli, args: &[String]) -> XtaskResult {
    let mut command = Command::new("cargo");
    command.current_dir(&cli.workspace_root);
    command.args(["run", "-q", "-p", &cli.package, "--"]);
    command.args(args);
    run_status(&mut command, cli.package.as_str())
}

/// Build the standard `auth login` arguments for a dev admin profile.
#[must_use]
pub fn admin_login_args(login: &AdminLogin) -> Vec<String> {
    vec![
        "--config".to_owned(),
        path_to_string(&login.config_path),
        "--profile".to_owned(),
        login.profile.clone(),
        "--quiet".to_owned(),
        "auth".to_owned(),
        "login".to_owned(),
        "--base-url".to_owned(),
        login.base_url.clone(),
        "--email".to_owned(),
        login.email.clone(),
        "--password".to_owned(),
        login.password.clone(),
    ]
}

/// Convert a path to a UTF-8 string for CLI arguments.
fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::{AdminLogin, admin_login_args};

    #[test]
    fn builds_login_arguments() {
        let login = AdminLogin::new(
            "/tmp/dev.toml",
            "dev",
            "http://localhost:8080",
            "admin@example.com",
            "secret",
        );

        let expected = [
            "--config",
            "/tmp/dev.toml",
            "--profile",
            "dev",
            "--quiet",
            "auth",
            "login",
            "--base-url",
            "http://localhost:8080",
            "--email",
            "admin@example.com",
            "--password",
            "secret",
        ]
        .map(str::to_string)
        .to_vec();

        assert_eq!(admin_login_args(&login), expected);
    }
}
