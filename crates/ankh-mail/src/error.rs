//! Errors for transactional mail operations.

use std::io;

use thiserror::Error;

/// Error type used across mail rendering and delivery.
#[derive(Debug, Error)]
pub enum Error {
    /// Public base URL is empty after normalization.
    #[error("public base url cannot be empty")]
    InvalidBaseUrl,

    /// Requested template name does not exist.
    #[error("template not found: {0}")]
    TemplateNotFound(String),

    /// Template contents are invalid.
    #[error("invalid template: {0}")]
    InvalidTemplate(String),

    /// Dev mail artifact on disk is malformed.
    #[error("invalid dev mail artifact: {0}")]
    InvalidDevMail(String),

    /// IO failure while writing or reading dev mail artifacts.
    #[error("mail io error: {0}")]
    Io(#[from] io::Error),
}
