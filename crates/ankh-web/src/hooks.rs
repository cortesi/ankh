//! Best-effort product hooks and test recorders.

use std::sync::{Arc, Mutex};

use ankh_types::{DeviceSessionId, NamespaceId, UserId};
use async_trait::async_trait;

use crate::admin::{AdminAudit, AdminAuditEvent};

/// Hook payload for removing a user from an organization namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrgMemberRemoved {
    /// Organization namespace name.
    pub namespace: String,
    /// Removed user ID.
    pub user_id: UserId,
}

/// Hook payload for namespace suspension state changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceStatusChanged {
    /// Namespace ID.
    pub namespace_id: NamespaceId,
    /// Namespace name.
    pub namespace: String,
    /// Whether the namespace is now suspended.
    pub suspended: bool,
    /// Edge-visible generation after the update.
    pub r#gen: i64,
}

/// Hook payload for namespace deletion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceDeleted {
    /// Namespace ID.
    pub namespace_id: NamespaceId,
    /// Namespace name.
    pub namespace: String,
}

/// Hook payload for one or more device-session revocations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSessionsRevoked {
    /// User that owned the revoked sessions.
    pub user_id: UserId,
    /// Revoked device session IDs.
    pub session_ids: Vec<DeviceSessionId>,
}

/// Product cleanup hooks dispatched after successful Ankh-owned mutations.
#[async_trait]
pub trait ProductHooks: Send + Sync {
    /// Called after an organization member has been removed.
    async fn on_org_member_removed(&self, payload: OrgMemberRemoved) -> Result<(), String>;

    /// Called after a namespace has been suspended.
    async fn on_namespace_suspended(&self, payload: NamespaceStatusChanged) -> Result<(), String>;

    /// Called after a namespace has been reinstated.
    async fn on_namespace_reinstated(&self, payload: NamespaceStatusChanged) -> Result<(), String>;

    /// Called before or after namespaces are deleted, depending on the owning product path.
    async fn on_namespaces_deleted(&self, payload: Vec<NamespaceDeleted>) -> Result<(), String>;

    /// Called after device sessions have been revoked.
    async fn on_device_sessions_revoked(
        &self,
        payload: DeviceSessionsRevoked,
    ) -> Result<(), String>;
}

/// No-op hook sink used by products that have no cleanup to run.
#[derive(Clone, Debug, Default)]
pub struct NoopProductHooks;

#[async_trait]
impl ProductHooks for NoopProductHooks {
    async fn on_org_member_removed(&self, _payload: OrgMemberRemoved) -> Result<(), String> {
        Ok(())
    }

    async fn on_namespace_suspended(&self, _payload: NamespaceStatusChanged) -> Result<(), String> {
        Ok(())
    }

    async fn on_namespace_reinstated(
        &self,
        _payload: NamespaceStatusChanged,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn on_namespaces_deleted(&self, _payload: Vec<NamespaceDeleted>) -> Result<(), String> {
        Ok(())
    }

    async fn on_device_sessions_revoked(
        &self,
        _payload: DeviceSessionsRevoked,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// Record-only audit sink used by Ankh web harness tests.
#[derive(Debug, Clone, Default)]
pub struct FakeAuditSink {
    /// Recorded audit event labels protected for cloneable test access.
    events: Arc<Mutex<Vec<String>>>,
}

impl FakeAuditSink {
    /// Create an empty audit sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an audit event label.
    pub fn record(&self, event: impl Into<String>) {
        let mut events = self.events.lock().expect("fake audit mutex poisoned");
        events.push(event.into());
    }

    /// Return and clear every recorded audit event.
    #[must_use]
    pub fn take_events(&self) -> Vec<String> {
        let mut events = self.events.lock().expect("fake audit mutex poisoned");
        events.drain(..).collect()
    }
}

#[async_trait]
impl AdminAudit for FakeAuditSink {
    async fn record(&self, event: AdminAuditEvent) -> Result<(), String> {
        self.record(format!(
            "{}:{}:{}",
            event.action,
            event.target_type,
            event.result.as_str()
        ));
        Ok(())
    }
}

/// Record-only hook sink used by Ankh web harness tests.
#[derive(Debug, Clone, Default)]
pub struct FakeHookRecorder {
    /// Recorded hook call labels protected for cloneable test access.
    calls: Arc<Mutex<Vec<String>>>,
}

impl FakeHookRecorder {
    /// Create an empty hook recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a hook call label.
    pub fn record(&self, call: impl Into<String>) {
        let mut calls = self.calls.lock().expect("fake hook mutex poisoned");
        calls.push(call.into());
    }

    /// Return and clear every recorded hook call.
    #[must_use]
    pub fn take_calls(&self) -> Vec<String> {
        let mut calls = self.calls.lock().expect("fake hook mutex poisoned");
        calls.drain(..).collect()
    }
}

#[async_trait]
impl ProductHooks for FakeHookRecorder {
    async fn on_org_member_removed(&self, payload: OrgMemberRemoved) -> Result<(), String> {
        self.record(format!(
            "org_member_removed:{}:{}",
            payload.namespace, payload.user_id
        ));
        Ok(())
    }

    async fn on_namespace_suspended(&self, payload: NamespaceStatusChanged) -> Result<(), String> {
        self.record(format!(
            "namespace_suspended:{}:{}",
            payload.namespace, payload.r#gen
        ));
        Ok(())
    }

    async fn on_namespace_reinstated(&self, payload: NamespaceStatusChanged) -> Result<(), String> {
        self.record(format!(
            "namespace_reinstated:{}:{}",
            payload.namespace, payload.r#gen
        ));
        Ok(())
    }

    async fn on_namespaces_deleted(&self, payload: Vec<NamespaceDeleted>) -> Result<(), String> {
        self.record(format!("namespaces_deleted:{}", payload.len()));
        Ok(())
    }

    async fn on_device_sessions_revoked(
        &self,
        payload: DeviceSessionsRevoked,
    ) -> Result<(), String> {
        let session_ids = payload
            .session_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.record(format!(
            "device_sessions_revoked:{}:{}",
            payload.user_id, session_ids
        ));
        Ok(())
    }
}
