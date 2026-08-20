//! Configuration file handling for Ankh admin CLIs.
//!
//! Stores profiles with base URLs and authentication tokens in a product-selected
//! TOML file.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Configuration file structure.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Default profile name to use.
    #[serde(default)]
    pub default_profile: Option<String>,

    /// Named profiles.
    #[serde(default, rename = "profile")]
    pub profiles: HashMap<String, Profile>,
}

/// A named profile with connection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Base URL for the admin API (e.g., "http://localhost:8080").
    pub base_url: String,

    /// Bearer token for authentication.
    #[serde(default)]
    pub token: Option<String>,

    /// Token expiration time.
    #[serde(default)]
    pub token_expires_at: Option<DateTime<Utc>>,
}

impl Config {
    /// Returns the default config file path for a product config filename.
    pub fn path(filename: &str) -> Result<PathBuf> {
        dirs::home_dir()
            .map(|p| p.join(filename))
            .ok_or_else(|| Error::Config("could not determine home directory".into()))
    }

    /// Load config from an explicit path, creating an empty config if it doesn't exist.
    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to an explicit path.
    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Get a profile by name, or the default profile if no name is specified.
    pub fn get_profile<'a>(&'a self, name: Option<&'a str>) -> Result<(&'a str, &'a Profile)> {
        let name = name
            .or(self.default_profile.as_deref())
            .ok_or(Error::NoProfile)?;

        let profile = self
            .profiles
            .get(name)
            .ok_or_else(|| Error::ProfileNotFound(name.to_string()))?;

        Ok((name, profile))
    }

    /// Get a mutable profile by name, or create it if it doesn't exist.
    pub fn get_or_create_profile(&mut self, name: &str, base_url: &str) -> &mut Profile {
        self.profiles
            .entry(name.to_string())
            .or_insert_with(|| Profile {
                base_url: base_url.to_string(),
                token: None,
                token_expires_at: None,
            })
    }

    /// Set the default profile.
    pub fn set_default_profile(&mut self, name: &str) {
        self.default_profile = Some(name.to_string());
    }
}

impl Profile {
    /// Get the token, returning an error if expired or missing.
    pub fn get_token(&self) -> Result<&str> {
        let token = self.token.as_deref().ok_or(Error::NoProfile)?;

        if let Some(expires) = &self.token_expires_at
            && expires <= &Utc::now()
        {
            return Err(Error::TokenExpired);
        }

        Ok(token)
    }
}
