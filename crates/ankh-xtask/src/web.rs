//! Simple key/value state-file helpers for local webdev wrappers.

use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

/// Persisted key/value webdev state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebdevState {
    /// Serialized values.
    values: BTreeMap<String, String>,
}

impl WebdevState {
    /// Create an empty webdev state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Set a state value.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.values.insert(key.to_string(), value.into());
    }

    /// Read a state value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Return the recorded base URL, if present.
    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        self.get("base_url")
    }

    /// Parse state from `key=value` lines.
    pub fn parse(contents: &str) -> Result<Self, Box<dyn Error>> {
        let mut state = Self::new();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("invalid webdev state line `{line}`").into());
            };
            state.set(key, value);
        }
        Ok(state)
    }

    /// Serialize the state as `key=value` lines.
    #[must_use]
    pub fn serialize(&self) -> String {
        let mut output = String::new();
        for (key, value) in &self.values {
            output.push_str(key);
            output.push('=');
            output.push_str(value);
            output.push('\n');
        }
        output
    }
}

/// File-backed webdev state.
#[derive(Debug, Clone)]
pub struct WebdevStateFile {
    /// State file path.
    path: PathBuf,
}

impl WebdevStateFile {
    /// Create a state-file handle.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Resolve the underlying state-file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load persisted state, returning `None` when no file exists.
    pub fn load(&self) -> Result<Option<WebdevState>, Box<dyn Error>> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(format!(
                    "failed to read webdev state {} ({err})",
                    self.path.display()
                )
                .into());
            }
        };
        WebdevState::parse(&contents).map(Some)
    }

    /// Write state to disk.
    pub fn write(&self, state: &WebdevState) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&self.path)?;
        file.write_all(state.serialize().as_bytes())?;
        Ok(())
    }

    /// Remove the state file if it exists.
    pub fn remove(&self) -> Result<(), Box<dyn Error>> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Load only the recorded base URL.
    pub fn base_url(&self) -> Result<Option<String>, Box<dyn Error>> {
        Ok(self
            .load()?
            .and_then(|state| state.base_url().map(str::to_string)))
    }
}

#[cfg(test)]
mod tests {
    use super::{WebdevState, WebdevStateFile};

    #[test]
    fn round_trips_webdev_state_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = WebdevStateFile::new(temp.path().join(".state/webdev.state"));
        let mut state = WebdevState::new();
        state.set("base_url", "http://localhost:52800");
        state.set("backend_url", "http://127.0.0.1:8081");

        file.write(&state).expect("write state");

        assert_eq!(
            file.base_url().expect("read base URL").as_deref(),
            Some("http://localhost:52800")
        );
        assert_eq!(file.load().expect("load state"), Some(state));

        file.remove().expect("remove state");
        assert!(file.load().expect("missing state").is_none());
    }

    #[test]
    fn rejects_malformed_state_lines() {
        let error = WebdevState::parse("not-a-pair").expect_err("invalid state should fail");
        assert!(error.to_string().contains("invalid webdev state line"));
    }
}
