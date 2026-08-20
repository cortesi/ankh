#![warn(missing_docs)]

//! Shared developer-task building blocks for Ankh and leaf workspaces.

/// Dev admin CLI helpers.
pub mod admin;
/// Command execution helpers.
pub mod command;
/// Frontend package-manager helpers.
pub mod frontend;
/// Local Postgres lifecycle helpers.
pub mod postgres;
/// Simple webdev state-file helpers.
pub mod web;
