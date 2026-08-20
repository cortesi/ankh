#![warn(missing_docs)]

//! Shared command-line administration plumbing for Ankh consumers.

mod cli;
mod client;
mod config;
mod error;
mod output;

pub use cli::{
    AuthCommand, CommonCommand, CommonRuntime, DeviceSessionsCommand, GlobalArgs, ListArgs,
    OrgInvitesCommand, OrgMembersCommand, OrgsCommand, ProductInfo, SessionsCommand,
    SettingsCommand, SysadminsCommand, UsersCommand, WaitlistCommand, get_client, run_common,
};
pub use client::{
    AdminClient, ListDeviceSessionsParams, ListOrgsParams, ListSessionsParams, ListSysadminsParams,
    ListUsersParams,
};
pub use config::{Config, Profile};
pub use error::{Error, Result};
pub use output::{Format, Render, info, print_cursor};

#[cfg(test)]
mod tests {
    //! Unit tests for product configuration and config-path resolution.

    use super::{Config, ProductInfo};

    /// `ProductInfo` exposes its identity and a sensible default base URL.
    #[test]
    fn product_info_exposes_identity_and_default_base_url() {
        let product = ProductInfo::new("ankh-cli", ".ankh.toml");
        assert_eq!(product.binary_name(), "ankh-cli");
        assert_eq!(product.config_filename(), ".ankh.toml");
        assert_eq!(product.default_base_url(), "http://localhost:8080");

        let custom =
            ProductInfo::with_default_base_url("ankh-cli", ".ankh.toml", "http://localhost:9000");
        assert_eq!(custom.default_base_url(), "http://localhost:9000");
    }

    /// The config path resolves to the product's config filename under the home directory.
    #[test]
    fn config_path_resolves_under_home_directory() {
        let product = ProductInfo::new("ankh-cli", ".ankh.toml");
        let path = Config::path(product.config_filename()).expect("resolve config path");
        assert!(path.is_absolute());
        assert!(path.ends_with(".ankh.toml"));
    }
}
