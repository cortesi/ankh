//! Shared Ankh web application state.

use std::sync::{Arc, Mutex};

use ankh_db::AnkhDbPool;

use crate::{
    admin::{AdminAudit, TracingAdminAudit},
    hooks::{NoopProductHooks, ProductHooks},
    mail::MailState,
};

/// Cookie behavior for browser web sessions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CookieConfig {
    /// Cookie name used for web sessions.
    pub session_cookie_name: String,
    /// Whether the session cookie should carry the Secure attribute.
    pub secure: bool,
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            session_cookie_name: "session".to_owned(),
            secure: true,
        }
    }
}

/// Device authorization route configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceAuthConfig {
    /// Browser login path used when a device authorize request has no web session.
    pub login_path: String,
}

impl Default for DeviceAuthConfig {
    fn default() -> Self {
        Self {
            login_path: "/login".to_owned(),
        }
    }
}

/// Shared Ankh web configuration.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnkhWebConfig {
    /// Cookie behavior.
    pub cookie: CookieConfig,
    /// Device authorization behavior.
    pub device_auth: DeviceAuthConfig,
}

/// Shared state required by the mountable Ankh public routers.
#[derive(Clone)]
pub struct AnkhWebState {
    /// Ankh identity database pool.
    db_pool: AnkhDbPool,
    /// Mail renderer and transport.
    mail: MailState,
    /// Runtime web configuration.
    config: AnkhWebConfig,
    /// Product cleanup hooks.
    hooks: Arc<dyn ProductHooks>,
    /// Admin audit event sink.
    admin_audit: Arc<dyn AdminAudit>,
    /// Best-effort hook failure messages recorded for inspection.
    hook_failures: Arc<Mutex<Vec<String>>>,
    /// Best-effort admin audit failure messages recorded for inspection.
    audit_failures: Arc<Mutex<Vec<String>>>,
}

impl AnkhWebState {
    /// Build shared web state with default config and no-op hooks.
    #[must_use]
    pub fn new(db_pool: AnkhDbPool, mail: MailState) -> Self {
        Self::with_config(db_pool, mail, AnkhWebConfig::default())
    }

    /// Build shared web state with explicit config and no-op hooks.
    #[must_use]
    pub fn with_config(db_pool: AnkhDbPool, mail: MailState, config: AnkhWebConfig) -> Self {
        Self {
            db_pool,
            mail,
            config,
            hooks: Arc::new(NoopProductHooks),
            admin_audit: Arc::new(TracingAdminAudit),
            hook_failures: Arc::new(Mutex::new(Vec::new())),
            audit_failures: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Return a copy of this state with product hooks installed.
    #[must_use]
    pub fn with_hooks(mut self, hooks: Arc<dyn ProductHooks>) -> Self {
        self.hooks = hooks;
        self
    }

    /// Return a copy of this state with an admin audit sink installed.
    #[must_use]
    pub fn with_admin_audit(mut self, admin_audit: Arc<dyn AdminAudit>) -> Self {
        self.admin_audit = admin_audit;
        self
    }

    /// Return the Ankh identity database pool.
    #[must_use]
    pub fn db_pool(&self) -> &AnkhDbPool {
        &self.db_pool
    }

    /// Return the shared mail state.
    #[must_use]
    pub fn mail(&self) -> &MailState {
        &self.mail
    }

    /// Return the runtime web configuration.
    #[must_use]
    pub fn config(&self) -> &AnkhWebConfig {
        &self.config
    }

    /// Return the installed product hooks.
    #[must_use]
    pub fn hooks(&self) -> Arc<dyn ProductHooks> {
        self.hooks.clone()
    }

    /// Return the installed admin audit sink.
    #[must_use]
    pub fn admin_audit(&self) -> Arc<dyn AdminAudit> {
        self.admin_audit.clone()
    }

    /// Record a best-effort hook failure.
    pub fn record_hook_failure(&self, label: impl Into<String>, error: impl Into<String>) {
        let mut failures = self
            .hook_failures
            .lock()
            .expect("hook failure mutex poisoned");
        failures.push(format!("{}: {}", label.into(), error.into()));
    }

    /// Record a best-effort admin audit failure.
    pub fn record_audit_failure(&self, label: impl Into<String>, error: impl Into<String>) {
        let mut failures = self
            .audit_failures
            .lock()
            .expect("audit failure mutex poisoned");
        failures.push(format!("{}: {}", label.into(), error.into()));
    }

    /// Return and clear recorded hook failures.
    #[must_use]
    pub fn take_hook_failures(&self) -> Vec<String> {
        let mut failures = self
            .hook_failures
            .lock()
            .expect("hook failure mutex poisoned");
        failures.drain(..).collect()
    }

    /// Return and clear recorded admin audit failures.
    #[must_use]
    pub fn take_audit_failures(&self) -> Vec<String> {
        let mut failures = self
            .audit_failures
            .lock()
            .expect("audit failure mutex poisoned");
        failures.drain(..).collect()
    }
}
