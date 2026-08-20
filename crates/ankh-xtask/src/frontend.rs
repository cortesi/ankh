//! Frontend package-manager helpers.

use std::{path::Path, process::Command};

use crate::command::{XtaskResult, binary_available, run_status};

/// Ensure a pnpm workspace has dependencies installed from its lockfile.
pub fn ensure_pnpm_dependencies(frontend_root: &Path) -> XtaskResult {
    let mut command = Command::new("pnpm");
    command
        .current_dir(frontend_root)
        .args(["install", "--frozen-lockfile"]);
    println!("-> pnpm install --frozen-lockfile");
    run_status(&mut command, "pnpm install --frozen-lockfile")
}

/// Run a pnpm command in `frontend_root`.
pub fn run_pnpm(frontend_root: &Path, args: &[&str]) -> XtaskResult {
    let mut command = Command::new("pnpm");
    command.current_dir(frontend_root).args(args);
    let label = format!("pnpm {}", args.join(" "));
    println!("-> {label}");
    run_status(&mut command, &label)
}

/// Run a pnpm script after installing dependencies from the checked-in lockfile.
pub fn run_pnpm_script_with_install(frontend_root: &Path, script: &str) -> XtaskResult {
    ensure_pnpm_dependencies(frontend_root)?;
    run_pnpm(frontend_root, &["run", script])
}

/// Run a pnpm command only when dependencies and pnpm are available.
pub fn run_pnpm_if_available(frontend_root: &Path, args: &[&str]) -> XtaskResult {
    let label = args.join(" ");
    if !frontend_root.join("node_modules").exists() {
        println!("(skipping frontend `{label}` - run `pnpm install` in frontend/)");
        return Ok(());
    }
    if !binary_available("pnpm") {
        println!("(skipping frontend `{label}` - pnpm not found in PATH)");
        return Ok(());
    }
    run_pnpm(frontend_root, args)
}
