//! Error types for shared Ankh admin CLIs.

use std::{io, result};

use thiserror::Error;
use toml::{de, ser};

/// CLI error type.
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// No profile configured.
    #[error("no profile configured. Run `auth login` first")]
    NoProfile,

    /// Profile not found.
    #[error("profile '{0}' not found")]
    ProfileNotFound(String),

    /// Token expired.
    #[error("token expired. Run `auth login` to re-authenticate")]
    TokenExpired,

    /// HTTP request error.
    #[error("request error: {0}")]
    Request(#[from] reqwest::Error),

    /// API error response.
    #[error("{message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error code from API.
        code: String,
        /// Error message from API.
        message: String,
    },

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// TOML parse error.
    #[error("config parse error: {0}")]
    TomlParse(#[from] de::Error),

    /// TOML serialize error.
    #[error("config write error: {0}")]
    TomlSerialize(#[from] ser::Error),

    /// JSON error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Response body could not be decoded.
    #[error("invalid response: {0}")]
    InvalidResponse(String),

    /// Prompt error.
    #[error("prompt error: {0}")]
    Prompt(#[from] inquire::InquireError),
}

/// CLI result type.
pub type Result<T> = result::Result<T, Error>;
