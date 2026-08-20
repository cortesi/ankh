//! Shared CLI command definitions and execution helpers for Ankh admin APIs.

use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

use ankh_types::admin::{
    AddMemberRequest, CreateOrgInviteRequest, CreateOrgRequest, ReleaseUserRequest, SetRoleRequest,
    TransferOwnershipRequest, UpdateOrgRequest,
};
use clap::{Args, Subcommand};

use crate::{
    client::{self, AdminClient},
    config::Config,
    error::Result,
    output::{self, Format, Render},
};

/// Product-specific CLI configuration supplied by leaf CLIs.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ProductInfo {
    /// Binary name used in help and diagnostics.
    binary_name: &'static str,
    /// Default config filename in the user's home directory.
    config_filename: &'static str,
    /// Default base URL used by login when no URL is supplied.
    default_base_url: &'static str,
}

impl ProductInfo {
    /// Build product metadata using the common local development base URL.
    #[must_use]
    pub const fn new(binary_name: &'static str, config_filename: &'static str) -> Self {
        Self {
            binary_name,
            config_filename,
            default_base_url: "http://localhost:8080",
        }
    }

    /// Build product metadata with an explicit default base URL.
    #[must_use]
    pub const fn with_default_base_url(
        binary_name: &'static str,
        config_filename: &'static str,
        default_base_url: &'static str,
    ) -> Self {
        Self {
            binary_name,
            config_filename,
            default_base_url,
        }
    }

    /// Return the binary name.
    #[must_use]
    pub const fn binary_name(self) -> &'static str {
        self.binary_name
    }

    /// Return the default config filename.
    #[must_use]
    pub const fn config_filename(self) -> &'static str {
        self.config_filename
    }

    /// Return the default base URL.
    #[must_use]
    pub const fn default_base_url(self) -> &'static str {
        self.default_base_url
    }
}

/// Global options shared across Ankh-consuming admin CLIs.
#[derive(Debug, Clone, Args)]
pub struct GlobalArgs {
    /// Base URL for the admin API (overrides config).
    #[arg(long, global = true)]
    pub base_url: Option<String>,

    /// Output format.
    #[arg(long, global = true, default_value = "table")]
    pub format: Format,

    /// Path to the CLI config file.
    #[arg(long = "config", global = true)]
    pub config_path: Option<PathBuf>,

    /// Profile to use.
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Suppress non-essential output.
    #[arg(long, short, global = true)]
    pub quiet: bool,

    /// Emit verbose connection details.
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Request trace ID for correlation.
    #[arg(long, global = true)]
    pub trace_id: Option<String>,
}

/// Shared arguments for list commands.
#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    /// Maximum number of items to return.
    #[arg(long, default_value = "50")]
    pub limit: i64,
    /// Pagination cursor.
    #[arg(long)]
    pub cursor: Option<String>,
}

impl GlobalArgs {
    /// Convert common flags into an executable shared runtime.
    #[must_use]
    pub fn into_runtime(self, product: ProductInfo) -> CommonRuntime {
        CommonRuntime {
            product,
            args: self,
        }
    }
}

/// Shared runtime used by common command handlers.
#[derive(Debug, Clone)]
pub struct CommonRuntime {
    /// Product metadata.
    product: ProductInfo,
    /// Common global arguments.
    args: GlobalArgs,
}

impl CommonRuntime {
    /// Build a shared runtime.
    #[must_use]
    pub fn new(product: ProductInfo, args: GlobalArgs) -> Self {
        Self { product, args }
    }

    /// Return the product metadata.
    #[must_use]
    pub const fn product(&self) -> ProductInfo {
        self.product
    }

    /// Resolve the config path for this invocation.
    pub fn config_path(&self) -> Result<PathBuf> {
        if let Some(path) = &self.config_path {
            Ok(path.clone())
        } else {
            Config::path(self.product.config_filename())
        }
    }

    /// Load config for this invocation and return the config path used.
    pub fn load_config(&self) -> Result<(Config, PathBuf)> {
        let path = self.config_path()?;
        let config = Config::load_from_path(&path)?;
        Ok((config, path))
    }
}

impl Deref for CommonRuntime {
    type Target = GlobalArgs;

    fn deref(&self) -> &Self::Target {
        &self.args
    }
}

/// Common top-level commands.
#[derive(Subcommand, Debug, Clone)]
pub enum CommonCommand {
    /// Authentication commands.
    Auth {
        /// Subcommand to run.
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// User management commands.
    Users {
        /// Subcommand to run.
        #[command(subcommand)]
        command: UsersCommand,
    },
    /// Session management commands.
    Sessions {
        /// Subcommand to run.
        #[command(subcommand)]
        command: SessionsCommand,
    },
    /// Device session management commands.
    DeviceSessions {
        /// Subcommand to run.
        #[command(subcommand)]
        command: DeviceSessionsCommand,
    },
    /// Sysadmin account commands.
    Sysadmins {
        /// Subcommand to run.
        #[command(subcommand)]
        command: SysadminsCommand,
    },
    /// Global settings commands.
    Settings {
        /// Subcommand to run.
        #[command(subcommand)]
        command: SettingsCommand,
    },
    /// Organization management commands.
    Orgs {
        /// Subcommand to run.
        #[command(subcommand)]
        command: OrgsCommand,
    },
}

/// Authentication commands.
#[derive(Subcommand, Debug, Clone)]
pub enum AuthCommand {
    /// Log in to the admin API.
    Login {
        /// Base URL for the admin API.
        #[arg(long)]
        base_url: Option<String>,
        /// Email address for authentication (optional; prompts if omitted).
        #[arg(long)]
        email: Option<String>,
        /// Password for authentication (optional; prompts if omitted).
        #[arg(long)]
        password: Option<String>,
    },
    /// Display the authenticated sysadmin identity.
    Whoami,
}

/// User management commands.
#[derive(Subcommand, Debug, Clone)]
pub enum UsersCommand {
    /// List users.
    List {
        /// Pagination controls.
        #[command(flatten)]
        list: ListArgs,
        /// Filter by email.
        #[arg(long)]
        email: Option<String>,
    },
    /// Get user details.
    Get {
        /// User ID.
        id: String,
    },
    /// Delete a user.
    Remove {
        /// User ID.
        id: String,
        /// Skip confirmation prompt.
        #[arg(long, short)]
        yes: bool,
    },
    /// Release a waitlisted user.
    Release {
        /// User email or ID.
        target: String,
    },
    /// Invite a user and bypass the waitlist.
    Invite {
        /// User email.
        email: String,
    },
}

/// Session management commands.
#[derive(Subcommand, Debug, Clone)]
pub enum SessionsCommand {
    /// List sessions.
    List {
        /// Pagination controls.
        #[command(flatten)]
        list: ListArgs,
        /// Filter by user ID.
        #[arg(long)]
        user_id: Option<String>,
        /// Filter by status (active, revoked, expired).
        #[arg(long)]
        status: Option<String>,
    },
    /// Revoke a session.
    Revoke {
        /// Session ID.
        id: String,
    },
}

/// Device session management commands.
#[derive(Subcommand, Debug, Clone)]
pub enum DeviceSessionsCommand {
    /// List device sessions.
    List {
        /// Pagination controls.
        #[command(flatten)]
        list: ListArgs,
        /// Filter by user ID.
        #[arg(long)]
        user_id: Option<String>,
        /// Filter by status (active, revoked, expired).
        #[arg(long)]
        status: Option<String>,
    },
    /// Revoke a device session.
    Revoke {
        /// Device session ID.
        id: String,
    },
}

/// Sysadmin account commands.
#[derive(Subcommand, Debug, Clone)]
pub enum SysadminsCommand {
    /// List sysadmin accounts.
    List {
        /// Pagination controls.
        #[command(flatten)]
        list: ListArgs,
    },
}

/// Settings commands.
#[derive(Subcommand, Debug, Clone)]
pub enum SettingsCommand {
    /// Waitlist settings.
    Waitlist {
        /// Subcommand to run.
        #[command(subcommand)]
        command: WaitlistCommand,
    },
}

/// Waitlist settings commands.
#[derive(Subcommand, Debug, Clone)]
pub enum WaitlistCommand {
    /// Show current waitlist setting.
    Status,
    /// Enable waitlist mode.
    Enable,
    /// Disable waitlist mode.
    Disable,
}

/// Organization management commands.
#[derive(Subcommand, Debug, Clone)]
pub enum OrgsCommand {
    /// List organizations.
    List {
        /// Pagination controls.
        #[command(flatten)]
        list: ListArgs,
    },
    /// Get organization details.
    Get {
        /// Organization ID.
        id: String,
    },
    /// Create an organization.
    Create {
        /// Organization name (namespace).
        name: String,
        /// Display name.
        #[arg(long)]
        display_name: Option<String>,
        /// Owner user ID.
        #[arg(long)]
        owner_id: String,
    },
    /// Update an organization.
    Update {
        /// Organization ID.
        id: String,
        /// Display name.
        #[arg(long)]
        display_name: Option<String>,
    },
    /// Delete an organization.
    Remove {
        /// Organization ID.
        id: String,
        /// Skip confirmation prompt.
        #[arg(long, short)]
        yes: bool,
    },
    /// Member management commands.
    Members {
        /// Subcommand to run.
        #[command(subcommand)]
        command: OrgMembersCommand,
    },
    /// Invite management commands.
    Invites {
        /// Subcommand to run.
        #[command(subcommand)]
        command: OrgInvitesCommand,
    },
    /// Transfer organization ownership.
    Transfer {
        /// Organization ID.
        id: String,
        /// New owner's user ID.
        #[arg(long)]
        new_owner_id: String,
        /// Skip confirmation prompt.
        #[arg(long, short)]
        yes: bool,
    },
}

/// Organization member commands.
#[derive(Subcommand, Debug, Clone)]
pub enum OrgMembersCommand {
    /// List organization members.
    List {
        /// Organization ID.
        org_id: String,
    },
    /// Add a member to an organization.
    Add {
        /// Organization ID.
        org_id: String,
        /// User ID to add.
        #[arg(long)]
        user_id: String,
        /// Role for the new member (admin or member).
        #[arg(long, default_value = "member")]
        role: String,
    },
    /// Remove a member from an organization.
    Remove {
        /// Organization ID.
        org_id: String,
        /// User ID to remove.
        user_id: String,
        /// Skip confirmation prompt.
        #[arg(long, short)]
        yes: bool,
    },
    /// Set a member's role.
    SetRole {
        /// Organization ID.
        org_id: String,
        /// User ID.
        user_id: String,
        /// New role (admin or member).
        #[arg(long)]
        role: String,
    },
}

/// Organization invite commands.
#[derive(Subcommand, Debug, Clone)]
pub enum OrgInvitesCommand {
    /// List pending organization invites.
    List {
        /// Organization ID.
        org_id: String,
    },
    /// Create an organization invite.
    Create {
        /// Organization ID.
        org_id: String,
        /// Email to invite.
        #[arg(long)]
        email: String,
    },
    /// Cancel an organization invite.
    Cancel {
        /// Organization ID.
        org_id: String,
        /// Invite ID.
        invite_id: String,
        /// Skip confirmation prompt.
        #[arg(long, short)]
        yes: bool,
    },
}

/// Execute a common command with the provided runtime configuration.
pub async fn run_common(runtime: CommonRuntime, command: CommonCommand) -> Result<()> {
    match command {
        CommonCommand::Auth { command } => run_auth(&runtime, command).await,
        CommonCommand::Users { command } => run_users(&runtime, command).await,
        CommonCommand::Sessions { command } => run_sessions(&runtime, command).await,
        CommonCommand::DeviceSessions { command } => run_device_sessions(&runtime, command).await,
        CommonCommand::Sysadmins { command } => run_sysadmins(&runtime, command).await,
        CommonCommand::Settings { command } => run_settings(&runtime, command).await,
        CommonCommand::Orgs { command } => run_orgs(&runtime, command).await,
    }
}

/// Execute authentication commands.
async fn run_auth(runtime: &CommonRuntime, command: AuthCommand) -> Result<()> {
    match command {
        AuthCommand::Login {
            base_url,
            email,
            password,
        } => cmd_auth_login(runtime, base_url, email, password).await,
        AuthCommand::Whoami => cmd_auth_whoami(runtime).await,
    }
}

/// Execute user management commands.
async fn run_users(runtime: &CommonRuntime, command: UsersCommand) -> Result<()> {
    match command {
        UsersCommand::List { list, email } => {
            cmd_users_list(runtime, list.limit, list.cursor, email).await
        }
        UsersCommand::Get { id } => cmd_users_get(runtime, &id).await,
        UsersCommand::Remove { id, yes } => cmd_users_remove(runtime, &id, yes).await,
        UsersCommand::Release { target } => cmd_users_release(runtime, &target).await,
        UsersCommand::Invite { email } => cmd_users_invite(runtime, &email).await,
    }
}

/// Execute session management commands.
async fn run_sessions(runtime: &CommonRuntime, command: SessionsCommand) -> Result<()> {
    match command {
        SessionsCommand::List {
            list,
            user_id,
            status,
        } => cmd_sessions_list(runtime, list.limit, list.cursor, user_id, status).await,
        SessionsCommand::Revoke { id } => cmd_sessions_revoke(runtime, &id).await,
    }
}

/// Execute device session management commands.
async fn run_device_sessions(
    runtime: &CommonRuntime,
    command: DeviceSessionsCommand,
) -> Result<()> {
    match command {
        DeviceSessionsCommand::List {
            list,
            user_id,
            status,
        } => cmd_device_sessions_list(runtime, list.limit, list.cursor, user_id, status).await,
        DeviceSessionsCommand::Revoke { id } => cmd_device_sessions_revoke(runtime, &id).await,
    }
}

/// Execute sysadmin account commands.
async fn run_sysadmins(runtime: &CommonRuntime, command: SysadminsCommand) -> Result<()> {
    match command {
        SysadminsCommand::List { list } => {
            cmd_sysadmins_list(runtime, list.limit, list.cursor).await
        }
    }
}

/// Execute global settings commands.
async fn run_settings(runtime: &CommonRuntime, command: SettingsCommand) -> Result<()> {
    match command {
        SettingsCommand::Waitlist { command } => run_waitlist(runtime, command).await,
    }
}

/// Execute waitlist-related commands.
async fn run_waitlist(runtime: &CommonRuntime, command: WaitlistCommand) -> Result<()> {
    match command {
        WaitlistCommand::Status => cmd_waitlist_status(runtime).await,
        WaitlistCommand::Enable => cmd_waitlist_update(runtime, true).await,
        WaitlistCommand::Disable => cmd_waitlist_update(runtime, false).await,
    }
}

/// Execute organization management commands.
async fn run_orgs(runtime: &CommonRuntime, command: OrgsCommand) -> Result<()> {
    match command {
        OrgsCommand::List { list } => cmd_orgs_list(runtime, list.limit, list.cursor).await,
        OrgsCommand::Get { id } => cmd_orgs_get(runtime, &id).await,
        OrgsCommand::Create {
            name,
            display_name,
            owner_id,
        } => cmd_orgs_create(runtime, &name, display_name.as_deref(), &owner_id).await,
        OrgsCommand::Update { id, display_name } => {
            cmd_orgs_update(runtime, &id, display_name.as_deref()).await
        }
        OrgsCommand::Remove { id, yes } => cmd_orgs_remove(runtime, &id, yes).await,
        OrgsCommand::Members { command } => run_org_members(runtime, command).await,
        OrgsCommand::Invites { command } => run_org_invites(runtime, command).await,
        OrgsCommand::Transfer {
            id,
            new_owner_id,
            yes,
        } => cmd_orgs_transfer(runtime, &id, &new_owner_id, yes).await,
    }
}

/// Execute organization membership commands.
async fn run_org_members(runtime: &CommonRuntime, command: OrgMembersCommand) -> Result<()> {
    match command {
        OrgMembersCommand::List { org_id } => cmd_orgs_members_list(runtime, &org_id).await,
        OrgMembersCommand::Add {
            org_id,
            user_id,
            role,
        } => cmd_orgs_members_add(runtime, &org_id, &user_id, &role).await,
        OrgMembersCommand::Remove {
            org_id,
            user_id,
            yes,
        } => cmd_orgs_members_remove(runtime, &org_id, &user_id, yes).await,
        OrgMembersCommand::SetRole {
            org_id,
            user_id,
            role,
        } => cmd_orgs_members_set_role(runtime, &org_id, &user_id, &role).await,
    }
}

/// Execute organization invite commands.
async fn run_org_invites(runtime: &CommonRuntime, command: OrgInvitesCommand) -> Result<()> {
    match command {
        OrgInvitesCommand::List { org_id } => cmd_orgs_invites_list(runtime, &org_id).await,
        OrgInvitesCommand::Create { org_id, email } => {
            cmd_orgs_invites_create(runtime, &org_id, &email).await
        }
        OrgInvitesCommand::Cancel {
            org_id,
            invite_id,
            yes,
        } => cmd_orgs_invites_cancel(runtime, &org_id, &invite_id, yes).await,
    }
}

/// Build an authenticated admin client based on runtime configuration.
pub fn get_client(runtime: &CommonRuntime) -> Result<AdminClient> {
    let (config, _) = runtime.load_config()?;
    let (profile_name, profile) = config.get_profile(runtime.profile.as_deref())?;

    let base_url = runtime.base_url.as_deref().unwrap_or(&profile.base_url);
    let token = profile.get_token()?;

    emit_connection_info(runtime, base_url, &runtime.config_path()?, profile_name);

    let mut client = AdminClient::new(base_url).with_token(token);
    if let Some(trace_id) = &runtime.trace_id {
        client = client.with_trace_id(trace_id);
    }

    Ok(client)
}

/// Run the auth login command.
async fn cmd_auth_login(
    runtime: &CommonRuntime,
    base_url_override: Option<String>,
    email: Option<String>,
    password: Option<String>,
) -> Result<()> {
    let email = resolve_email(email)?;
    let password = resolve_password(password)?;

    let base_url = base_url_override
        .or_else(|| runtime.base_url.clone())
        .unwrap_or_else(|| runtime.product().default_base_url().to_string());
    emit_connection_info(
        runtime,
        &base_url,
        &runtime.config_path()?,
        runtime.profile.as_deref().unwrap_or("default"),
    );

    let mut client = AdminClient::new(&base_url);
    if let Some(trace_id) = &runtime.trace_id {
        client = client.with_trace_id(trace_id);
    }

    let response = client.login(&email, &password).await?;

    let (mut config, config_path) = runtime.load_config()?;
    let profile_name = runtime.profile.as_deref().unwrap_or("default");

    let profile = config.get_or_create_profile(profile_name, &base_url);
    profile.token = Some(response.token);
    profile.token_expires_at = Some(response.expires_at);
    profile.base_url = base_url;

    if config.default_profile.is_none() {
        config.set_default_profile(profile_name);
    }

    config.save_to_path(&config_path)?;

    output::info(
        runtime.quiet,
        &format!(
            "Logged in as {} ({})",
            response.sysadmin.email, response.sysadmin.id
        ),
    );
    output::info(
        runtime.quiet,
        &format!(
            "Token expires at {}",
            response.expires_at.format("%Y-%m-%d %H:%M:%S")
        ),
    );

    Ok(())
}

/// Run the auth whoami command.
async fn cmd_auth_whoami(runtime: &CommonRuntime) -> Result<()> {
    let client = get_client(runtime)?;
    let response = client.whoami().await?;

    response.sysadmin.render(runtime.format);

    Ok(())
}

/// Run the users list command.
async fn cmd_users_list(
    runtime: &CommonRuntime,
    limit: i64,
    cursor: Option<String>,
    email: Option<String>,
) -> Result<()> {
    let client = get_client(runtime)?;
    let params = client::ListUsersParams {
        limit: Some(limit),
        cursor,
        email,
    };
    let response = client.list_users(&params).await?;

    response.users.render(runtime.format);
    output::print_cursor(response.next_cursor.as_deref(), runtime.quiet);

    Ok(())
}

/// Run the users get command.
async fn cmd_users_get(runtime: &CommonRuntime, id: &str) -> Result<()> {
    let client = get_client(runtime)?;
    let response = client.get_user(id).await?;

    response.render(runtime.format);

    Ok(())
}

/// Run the users remove command.
async fn cmd_users_remove(runtime: &CommonRuntime, id: &str, skip_confirm: bool) -> Result<()> {
    if !skip_confirm {
        let confirm = inquire::Confirm::new(&format!("Delete user {id}?"))
            .with_default(false)
            .prompt()?;
        if !confirm {
            output::info(runtime.quiet, "Cancelled");
            return Ok(());
        }
    }

    let client = get_client(runtime)?;
    client.delete_user(id).await?;

    output::info(runtime.quiet, &format!("User {id} deleted"));

    Ok(())
}

/// Run the users release command.
async fn cmd_users_release(runtime: &CommonRuntime, target: &str) -> Result<()> {
    let client = get_client(runtime)?;
    let request = if target.contains('@') {
        ReleaseUserRequest {
            id: None,
            email: Some(target.to_string()),
        }
    } else {
        ReleaseUserRequest {
            id: Some(target.to_string()),
            email: None,
        }
    };

    let response = client.release_user(&request).await?;

    response.render(runtime.format);

    Ok(())
}

/// Run the users invite command.
async fn cmd_users_invite(runtime: &CommonRuntime, email: &str) -> Result<()> {
    let client = get_client(runtime)?;
    let response = client.invite_user(email).await?;

    response.render(runtime.format);

    Ok(())
}

/// Run the sessions list command.
async fn cmd_sessions_list(
    runtime: &CommonRuntime,
    limit: i64,
    cursor: Option<String>,
    user_id: Option<String>,
    status: Option<String>,
) -> Result<()> {
    let client = get_client(runtime)?;
    let params = client::ListSessionsParams {
        limit: Some(limit),
        cursor,
        user_id,
        status,
    };
    let response = client.list_sessions(&params).await?;

    response.sessions.render(runtime.format);
    output::print_cursor(response.next_cursor.as_deref(), runtime.quiet);

    Ok(())
}

/// Run the sessions revoke command.
async fn cmd_sessions_revoke(runtime: &CommonRuntime, id: &str) -> Result<()> {
    let client = get_client(runtime)?;
    client.revoke_session(id).await?;

    output::info(runtime.quiet, &format!("Session {id} revoked"));

    Ok(())
}

/// Run the device sessions list command.
async fn cmd_device_sessions_list(
    runtime: &CommonRuntime,
    limit: i64,
    cursor: Option<String>,
    user_id: Option<String>,
    status: Option<String>,
) -> Result<()> {
    let client = get_client(runtime)?;
    let params = client::ListDeviceSessionsParams {
        limit: Some(limit),
        cursor,
        user_id,
        status,
    };
    let response = client.list_device_sessions(&params).await?;

    response.sessions.render(runtime.format);
    output::print_cursor(response.next_cursor.as_deref(), runtime.quiet);

    Ok(())
}

/// Run the device sessions revoke command.
async fn cmd_device_sessions_revoke(runtime: &CommonRuntime, id: &str) -> Result<()> {
    let client = get_client(runtime)?;
    client.revoke_device_session(id).await?;

    output::info(runtime.quiet, &format!("Device session {id} revoked"));

    Ok(())
}

/// Run the sysadmins list command.
async fn cmd_sysadmins_list(
    runtime: &CommonRuntime,
    limit: i64,
    cursor: Option<String>,
) -> Result<()> {
    let client = get_client(runtime)?;
    let params = client::ListSysadminsParams {
        limit: Some(limit),
        cursor,
    };
    let response = client.list_sysadmins(&params).await?;

    response.sysadmins.render(runtime.format);
    output::print_cursor(response.next_cursor.as_deref(), runtime.quiet);

    Ok(())
}

/// Run the waitlist status command.
async fn cmd_waitlist_status(runtime: &CommonRuntime) -> Result<()> {
    let client = get_client(runtime)?;
    let response = client.get_settings().await?;

    response.render(runtime.format);

    Ok(())
}

/// Run the waitlist update command.
async fn cmd_waitlist_update(runtime: &CommonRuntime, enabled: bool) -> Result<()> {
    let client = get_client(runtime)?;
    let response = client.set_waitlist_enabled(enabled).await?;

    response.render(runtime.format);

    Ok(())
}

// --- Organization commands ---

/// Run the orgs list command.
async fn cmd_orgs_list(runtime: &CommonRuntime, limit: i64, cursor: Option<String>) -> Result<()> {
    let client = get_client(runtime)?;
    let params = client::ListOrgsParams {
        limit: Some(limit),
        cursor,
    };
    let response = client.list_orgs(&params).await?;

    response.orgs.render(runtime.format);
    output::print_cursor(response.next_cursor.as_deref(), runtime.quiet);

    Ok(())
}

/// Run the orgs get command.
async fn cmd_orgs_get(runtime: &CommonRuntime, id: &str) -> Result<()> {
    let client = get_client(runtime)?;
    let response = client.get_org(id).await?;

    response.render(runtime.format);

    Ok(())
}

/// Run the orgs create command.
async fn cmd_orgs_create(
    runtime: &CommonRuntime,
    name: &str,
    display_name: Option<&str>,
    owner_id: &str,
) -> Result<()> {
    let client = get_client(runtime)?;
    let request = CreateOrgRequest {
        name: name.to_string(),
        display_name: display_name.map(|s| s.to_string()),
        owner_id: owner_id.to_string(),
    };
    let response = client.create_org(&request).await?;

    response.render(runtime.format);

    Ok(())
}

/// Run the orgs update command.
async fn cmd_orgs_update(
    runtime: &CommonRuntime,
    id: &str,
    display_name: Option<&str>,
) -> Result<()> {
    let client = get_client(runtime)?;
    let request = UpdateOrgRequest {
        display_name: display_name.map(|s| s.to_string()),
    };
    client.update_org(id, &request).await?;

    output::info(runtime.quiet, &format!("Organization {id} updated"));

    Ok(())
}

/// Run the orgs remove command.
async fn cmd_orgs_remove(runtime: &CommonRuntime, id: &str, skip_confirm: bool) -> Result<()> {
    if !skip_confirm {
        let confirm = inquire::Confirm::new(&format!("Delete organization {id}?"))
            .with_default(false)
            .prompt()?;
        if !confirm {
            output::info(runtime.quiet, "Cancelled");
            return Ok(());
        }
    }

    let client = get_client(runtime)?;
    client.delete_org(id).await?;

    output::info(runtime.quiet, &format!("Organization {id} deleted"));

    Ok(())
}

/// Run the orgs members list command.
async fn cmd_orgs_members_list(runtime: &CommonRuntime, org_id: &str) -> Result<()> {
    let client = get_client(runtime)?;
    let response = client.list_org_members(org_id).await?;

    response.render(runtime.format);

    Ok(())
}

/// Run the orgs members add command.
async fn cmd_orgs_members_add(
    runtime: &CommonRuntime,
    org_id: &str,
    user_id: &str,
    role: &str,
) -> Result<()> {
    let client = get_client(runtime)?;
    let request = AddMemberRequest {
        user_id: user_id.to_string(),
        role: role.to_string(),
    };
    client.add_org_member(org_id, &request).await?;

    output::info(
        runtime.quiet,
        &format!("User {user_id} added to organization {org_id} as {role}"),
    );

    Ok(())
}

/// Run the orgs members remove command.
async fn cmd_orgs_members_remove(
    runtime: &CommonRuntime,
    org_id: &str,
    user_id: &str,
    skip_confirm: bool,
) -> Result<()> {
    if !skip_confirm {
        let confirm = inquire::Confirm::new(&format!(
            "Remove user {user_id} from organization {org_id}?"
        ))
        .with_default(false)
        .prompt()?;
        if !confirm {
            output::info(runtime.quiet, "Cancelled");
            return Ok(());
        }
    }

    let client = get_client(runtime)?;
    client.remove_org_member(org_id, user_id).await?;

    output::info(
        runtime.quiet,
        &format!("User {user_id} removed from organization {org_id}"),
    );

    Ok(())
}

/// Run the orgs members set role command.
async fn cmd_orgs_members_set_role(
    runtime: &CommonRuntime,
    org_id: &str,
    user_id: &str,
    role: &str,
) -> Result<()> {
    let client = get_client(runtime)?;
    let request = SetRoleRequest {
        role: role.to_string(),
    };
    client.set_member_role(org_id, user_id, &request).await?;

    output::info(
        runtime.quiet,
        &format!("User {user_id} role set to {role} in organization {org_id}"),
    );

    Ok(())
}

/// Run the orgs invites list command.
async fn cmd_orgs_invites_list(runtime: &CommonRuntime, org_id: &str) -> Result<()> {
    let client = get_client(runtime)?;
    let response = client.list_org_invites(org_id).await?;

    response.invites.render(runtime.format);

    Ok(())
}

/// Run the orgs invites create command.
async fn cmd_orgs_invites_create(runtime: &CommonRuntime, org_id: &str, email: &str) -> Result<()> {
    let client = get_client(runtime)?;
    let request = CreateOrgInviteRequest {
        email: email.to_string(),
    };
    let response = client.create_org_invite(org_id, &request).await?;

    response.render(runtime.format);

    Ok(())
}

/// Run the orgs invites cancel command.
async fn cmd_orgs_invites_cancel(
    runtime: &CommonRuntime,
    org_id: &str,
    invite_id: &str,
    skip_confirm: bool,
) -> Result<()> {
    if !skip_confirm {
        let confirm = inquire::Confirm::new(&format!("Cancel invite {invite_id}?"))
            .with_default(false)
            .prompt()?;
        if !confirm {
            output::info(runtime.quiet, "Cancelled");
            return Ok(());
        }
    }

    let client = get_client(runtime)?;
    client.cancel_org_invite(org_id, invite_id).await?;

    output::info(runtime.quiet, &format!("Invite {invite_id} cancelled"));

    Ok(())
}

/// Run the orgs transfer command.
async fn cmd_orgs_transfer(
    runtime: &CommonRuntime,
    id: &str,
    new_owner_id: &str,
    skip_confirm: bool,
) -> Result<()> {
    if !skip_confirm {
        let confirm = inquire::Confirm::new(&format!(
            "Transfer ownership of organization {id} to user {new_owner_id}?"
        ))
        .with_default(false)
        .prompt()?;
        if !confirm {
            output::info(runtime.quiet, "Cancelled");
            return Ok(());
        }
    }

    let client = get_client(runtime)?;
    let request = TransferOwnershipRequest {
        new_owner_id: new_owner_id.to_string(),
    };
    client.transfer_org_ownership(id, &request).await?;

    output::info(
        runtime.quiet,
        &format!("Organization {id} ownership transferred to {new_owner_id}"),
    );

    Ok(())
}

/// Resolve the email argument or prompt the user.
fn resolve_email(email: Option<String>) -> Result<String> {
    match email {
        Some(email) => Ok(email),
        None => Ok(inquire::Text::new("Email:").prompt()?),
    }
}

/// Resolve the password argument or prompt the user.
fn resolve_password(password: Option<String>) -> Result<String> {
    match password {
        Some(password) => Ok(password),
        None => Ok(inquire::Password::new("Password:")
            .without_confirmation()
            .prompt()?),
    }
}

/// Emit verbose connection details for the current invocation.
fn emit_connection_info(
    runtime: &CommonRuntime,
    base_url: &str,
    config_path: &Path,
    profile_name: &str,
) {
    output::verbose(
        runtime.verbose,
        &format!(
            "Connecting to {} (profile {}, config {})",
            base_url,
            profile_name,
            config_path.display()
        ),
    );
}
