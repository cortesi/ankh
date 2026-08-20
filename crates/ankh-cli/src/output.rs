//! Output formatting for Ankh admin CLIs.
//!
//! Provides table and JSON output renderers.

use std::slice::from_ref;

use ankh_types::admin::{
    CreateOrgInviteResponse, DeviceSessionSummary, InviteAction, InviteUserResponse,
    ListMembersResponse, OrgDetail, OrgInvite, OrgMember, OrgSummary, ReleaseUserResponse,
    SessionSummary, SettingsResponse, SysadminSummary, UserDetail, UserSummary,
};
use chrono::{DateTime, Utc};
use comfy_table::{Cell, Table};
use serde::Serialize;

/// Output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// Table output (default).
    #[default]
    Table,
    /// JSON output.
    Json,
}

/// Trait for types that can be rendered.
pub trait Render {
    /// Render the value in the specified format.
    fn render(&self, format: Format);
    /// Render the value to a string in the specified format.
    fn render_to_string(&self, format: Format) -> String;
}

impl<T: Serialize + RenderTable> Render for T {
    fn render(&self, format: Format) {
        println!("{}", self.render_to_string(format));
    }

    fn render_to_string(&self, format: Format) -> String {
        match format {
            Format::Table => self.render_table_to_string(),
            Format::Json => serde_json::to_string_pretty(self).expect("serialize output as JSON"),
        }
    }
}

/// Trait for types that can be rendered as a table.
pub trait RenderTable {
    /// Build the table representation.
    fn table(&self) -> Table;

    /// Render as a table.
    fn render_table(&self) {
        println!("{}", self.render_table_to_string());
    }

    /// Render the table representation to a string.
    fn render_table_to_string(&self) -> String {
        self.table().to_string()
    }
}

/// Format a timestamp as `YYYY-MM-DD HH:MM`.
fn fmt_ts_short(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M").to_string()
}

/// Format a timestamp as `YYYY-MM-DD HH:MM:SS`.
fn fmt_ts_long(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Short-form timestamp, or `-` if absent.
fn fmt_ts_opt_short(ts: Option<DateTime<Utc>>) -> String {
    ts.map(fmt_ts_short).unwrap_or_else(|| "-".to_string())
}

/// Long-form timestamp, or `-` if absent.
fn fmt_ts_opt_long(ts: Option<DateTime<Utc>>) -> String {
    ts.map(fmt_ts_long).unwrap_or_else(|| "-".to_string())
}

/// Builder for two-column key/value tables.
struct KvTable(Table);

impl KvTable {
    /// Create an empty builder.
    fn new() -> Self {
        Self(Table::new())
    }

    /// Add a key/value row. `value` is any `ToString`, including `&String`, `&str`, or `bool`.
    fn row<V: ToString>(mut self, key: &str, value: V) -> Self {
        self.0.add_row(vec![Cell::new(key), Cell::new(value)]);
        self
    }

    /// Return the built table.
    fn into_table(self) -> Table {
        self.0
    }
}

impl RenderTable for Vec<UserSummary> {
    fn table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["ID", "Username", "Email", "Created", "Verified"]);
        for user in self {
            table.add_row(vec![
                Cell::new(&user.id),
                Cell::new(&user.username),
                Cell::new(&user.email),
                Cell::new(fmt_ts_short(user.created_at)),
                Cell::new(fmt_ts_opt_short(user.verified_at)),
            ]);
        }
        table
    }
}

impl RenderTable for UserDetail {
    fn table(&self) -> Table {
        KvTable::new()
            .row("ID", &self.id)
            .row("Username", &self.username)
            .row("Email", &self.email)
            .row("Created", fmt_ts_long(self.created_at))
            .row("Verified", fmt_ts_opt_long(self.verified_at))
            .row("Last Session", fmt_ts_opt_long(self.last_session_at))
            .into_table()
    }
}

impl RenderTable for Vec<SessionSummary> {
    fn table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec![
            "ID",
            "User",
            "Status",
            "Created",
            "Last Seen",
            "Expires",
        ]);
        for session in self {
            table.add_row(vec![
                Cell::new(&session.id),
                Cell::new(&session.user_email),
                Cell::new(&session.status),
                Cell::new(fmt_ts_short(session.created_at)),
                Cell::new(fmt_ts_short(session.last_seen_at)),
                Cell::new(fmt_ts_short(session.expires_at)),
            ]);
        }
        table
    }
}

impl RenderTable for Vec<DeviceSessionSummary> {
    fn table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec![
            "ID",
            "User",
            "Device",
            "Platform",
            "Status",
            "Created",
            "Last Used",
            "Expires",
        ]);
        for session in self {
            table.add_row(vec![
                Cell::new(&session.id),
                Cell::new(&session.user_email),
                Cell::new(&session.device_name),
                Cell::new(&session.platform),
                Cell::new(&session.status),
                Cell::new(fmt_ts_short(session.created_at)),
                Cell::new(fmt_ts_short(session.last_used_at)),
                Cell::new(fmt_ts_short(session.expires_at)),
            ]);
        }
        table
    }
}

/// Build the sysadmin listing table from a sequence of summaries.
fn sysadmin_table<'a>(admins: impl IntoIterator<Item = &'a SysadminSummary>) -> Table {
    let mut table = Table::new();
    table.set_header(vec!["ID", "Email", "Created", "Last Login"]);
    for admin in admins {
        table.add_row(vec![
            Cell::new(&admin.id),
            Cell::new(&admin.email),
            Cell::new(fmt_ts_short(admin.created_at)),
            Cell::new(fmt_ts_opt_short(admin.last_login_at)),
        ]);
    }
    table
}

impl RenderTable for Vec<SysadminSummary> {
    fn table(&self) -> Table {
        sysadmin_table(self)
    }
}

impl RenderTable for SysadminSummary {
    fn table(&self) -> Table {
        sysadmin_table(from_ref(self))
    }
}

impl RenderTable for ReleaseUserResponse {
    fn table(&self) -> Table {
        KvTable::new().row("Released", &self.email).into_table()
    }
}

impl RenderTable for InviteUserResponse {
    fn table(&self) -> Table {
        let action = match self.action {
            InviteAction::Invited => "invited",
            InviteAction::Released => "released",
            InviteAction::AlreadyActive => "already_active",
        };
        KvTable::new()
            .row("Email", &self.email)
            .row("Action", action)
            .into_table()
    }
}

impl RenderTable for SettingsResponse {
    fn table(&self) -> Table {
        KvTable::new()
            .row("Waitlist Enabled", self.waitlist_enabled)
            .into_table()
    }
}

/// Print a message if not in quiet mode.
pub fn info(quiet: bool, msg: &str) {
    if !quiet {
        println!("{msg}");
    }
}

/// Print a verbose message if verbose mode is enabled.
pub fn verbose(verbose: bool, msg: &str) {
    if verbose {
        println!("{msg}");
    }
}

/// Print pagination info.
pub fn print_cursor(cursor: Option<&str>, quiet: bool) {
    if let Some(cursor) = cursor
        && !quiet
    {
        println!("\nNext cursor: {cursor}");
    }
}

// --- Organization renderers ---

impl RenderTable for Vec<OrgSummary> {
    fn table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["ID", "Name", "Display Name", "Created"]);
        for org in self {
            table.add_row(vec![
                Cell::new(&org.id),
                Cell::new(&org.name),
                Cell::new(org.display_name.as_deref().unwrap_or("-")),
                Cell::new(fmt_ts_short(org.created_at)),
            ]);
        }
        table
    }
}

impl RenderTable for OrgDetail {
    fn table(&self) -> Table {
        KvTable::new()
            .row("ID", &self.id)
            .row("Name", &self.name)
            .row("Display Name", self.display_name.as_deref().unwrap_or("-"))
            .row("Created By", self.created_by.as_deref().unwrap_or("-"))
            .row("Namespace ID", &self.namespace_id)
            .row("Namespace Status", &self.namespace_status)
            .row("Namespace Generation", self.namespace_gen)
            .row("Updated", fmt_ts_long(self.updated_at))
            .row("Created", fmt_ts_long(self.created_at))
            .into_table()
    }
}

impl RenderTable for Vec<OrgMember> {
    fn table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["User ID", "Username", "Email", "Role", "Added"]);
        for member in self {
            table.add_row(vec![
                Cell::new(&member.user_id),
                Cell::new(&member.username),
                Cell::new(&member.email),
                Cell::new(&member.role),
                Cell::new(fmt_ts_short(member.created_at)),
            ]);
        }
        table
    }
}

impl RenderTable for ListMembersResponse {
    fn table(&self) -> Table {
        self.members.table()
    }
}

impl RenderTable for OrgMember {
    fn table(&self) -> Table {
        KvTable::new()
            .row("User ID", &self.user_id)
            .row("Username", &self.username)
            .row("Email", &self.email)
            .row("Role", &self.role)
            .row("Added", fmt_ts_long(self.created_at))
            .into_table()
    }
}

impl RenderTable for Vec<OrgInvite> {
    fn table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["ID", "Email", "Created", "Expires"]);
        for invite in self {
            table.add_row(vec![
                Cell::new(&invite.id),
                Cell::new(&invite.email),
                Cell::new(fmt_ts_short(invite.created_at)),
                Cell::new(fmt_ts_short(invite.expires_at)),
            ]);
        }
        table
    }
}

impl RenderTable for CreateOrgInviteResponse {
    fn table(&self) -> Table {
        KvTable::new()
            .row("ID", &self.id)
            .row("Email", &self.email)
            .row("Token", &self.token)
            .into_table()
    }
}
