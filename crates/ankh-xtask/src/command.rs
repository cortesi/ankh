//! Command execution and workspace helper APIs.

use std::{
    env,
    error::Error,
    ffi::OsStr,
    future::Future,
    path::{MAIN_SEPARATOR, Path, PathBuf},
    process::{Command, ExitStatus},
};

use tokio::runtime::Builder as TokioRuntimeBuilder;

/// Result type used by xtask helpers.
pub type XtaskResult<T = ()> = Result<T, Box<dyn Error>>;

/// Resolve a workspace root from an xtask crate manifest directory.
#[must_use]
pub fn workspace_root_from_manifest(manifest_dir: &str) -> PathBuf {
    let path = PathBuf::from(manifest_dir).join("..").join("..");
    path.canonicalize().unwrap_or(path)
}

/// Execute a prepared command, returning a friendly error for non-zero status.
pub fn run_status(command: &mut Command, label: &str) -> XtaskResult {
    let status = command.status()?;
    ensure_success(status, label)
}

/// Execute `cargo` in `workspace_root` with optional passthrough arguments.
pub fn exec_cargo(
    workspace_root: &Path,
    base_args: &[&str],
    passthrough: &[String],
) -> XtaskResult {
    let mut command = Command::new("cargo");
    command.current_dir(workspace_root);
    command.args(base_args);
    command.args(passthrough);

    let label = format!("cargo {}", format_args(base_args, passthrough));
    println!("-> {label}");
    run_status(&mut command, &label)
}

/// Run rustfmt using `rustfmt-nightly.toml` when the workspace provides it.
pub fn run_rustfmt(workspace_root: &Path) -> XtaskResult {
    let rustfmt_config = workspace_root.join("rustfmt-nightly.toml");
    let mut command = Command::new("cargo");
    command.current_dir(workspace_root);

    let label = if rustfmt_config.exists() {
        command.args([
            "+nightly",
            "fmt",
            "--all",
            "--",
            "--config-path",
            rustfmt_config.to_string_lossy().as_ref(),
        ]);
        format!(
            "cargo +nightly fmt --all -- --config-path {}",
            rustfmt_config.display()
        )
    } else {
        command.args(["+nightly", "fmt", "--all"]);
        "cargo +nightly fmt --all".to_string()
    };

    println!("-> {label}");
    run_status(&mut command, &label)
}

/// Drive an async action that returns an error from a synchronous xtask context.
pub fn run_async_result<T, E>(future: impl Future<Output = Result<T, E>>) -> XtaskResult<T>
where
    E: Error + 'static,
{
    let runtime = TokioRuntimeBuilder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(future).map_err(|err| err.into())
}

/// Return true if `name` is available on `PATH`.
#[must_use]
pub fn binary_available(name: &str) -> bool {
    if name.contains(MAIN_SEPARATOR) {
        return Path::new(name).is_file();
    }

    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|dir| executable_candidate_exists(&dir, name))
    })
}

/// Ensure every binary in `names` is present on `PATH`.
pub fn require_bins(names: &[&str]) -> XtaskResult {
    for name in names {
        if !binary_available(name) {
            return Err(format!("required binary `{name}` not found in PATH").into());
        }
    }
    Ok(())
}

/// Render command arguments for logging.
#[must_use]
pub fn format_args(base_args: &[&str], passthrough: &[String]) -> String {
    let mut parts: Vec<String> = base_args.iter().map(|arg| (*arg).to_owned()).collect();
    parts.extend(passthrough.iter().cloned());
    parts.join(" ")
}

/// Convert a non-successful exit status into a friendly error message.
pub fn ensure_success(status: ExitStatus, label: &str) -> XtaskResult {
    if status.success() {
        return Ok(());
    }
    Err(format!("`{label}` failed with status {status}").into())
}

/// Return true if a candidate executable path exists.
fn executable_candidate_exists(dir: &Path, name: &str) -> bool {
    dir.join(name).is_file() || windows_executable_candidates(dir, name).any(|path| path.is_file())
}

/// Candidate executable paths on Windows.
fn windows_executable_candidates<'a>(
    dir: &'a Path,
    name: &'a str,
) -> impl Iterator<Item = PathBuf> + 'a {
    env::var_os("PATHEXT")
        .into_iter()
        .flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .map(move |extension| {
            dir.join(format!("{name}{}", os_str_to_suffix(extension.as_os_str())))
        })
}

/// Convert an OS string path extension into a suffix.
fn os_str_to_suffix(value: &OsStr) -> String {
    let text = value.to_string_lossy();
    if text.starts_with('.') {
        text.into_owned()
    } else {
        format!(".{text}")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{format_args, workspace_root_from_manifest};

    #[test]
    fn formats_base_and_passthrough_arguments() {
        let passthrough = vec!["--".to_string(), "filter".to_string()];
        assert_eq!(
            format_args(&["test", "--all"], &passthrough),
            "test --all -- filter"
        );
    }

    #[test]
    fn resolves_workspace_root_from_manifest_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest_dir = temp.path().join("crates/xtask");
        fs::create_dir_all(&manifest_dir).expect("manifest dir");

        let root = workspace_root_from_manifest(manifest_dir.to_str().expect("utf-8 path"));

        assert_eq!(root, temp.path().canonicalize().expect("canonical tempdir"));
    }
}
