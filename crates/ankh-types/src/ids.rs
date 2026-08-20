//! Shared identifier and storage enum types.

use std::{fmt, result, str::FromStr};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Generates a UUID-backed identifier newtype with standard trait implementations.
macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[ts(type = "string")]
        pub struct $name(pub Uuid);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> result::Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }
    };
}

uuid_id!(
    /// Unique identifier for a sysadmin account.
    SysadminId
);

uuid_id!(
    /// Unique identifier for a sysadmin bearer token.
    SysadminTokenId
);

uuid_id!(
    /// Unique identifier for a user.
    UserId
);

uuid_id!(
    /// Unique identifier for a browser web session.
    SessionId
);

uuid_id!(
    /// Unique identifier for a namespace owned by a user or org.
    NamespaceId
);

uuid_id!(
    /// Unique identifier for an organization.
    OrgId
);

uuid_id!(
    /// Unique identifier for an organization invite.
    OrgInviteId
);

uuid_id!(
    /// Unique identifier for an account invite.
    InviteId
);

uuid_id!(
    /// Unique identifier for a one-time identity token.
    TokenId
);

uuid_id!(
    /// Unique identifier for a device authorization grant.
    DeviceAuthGrantId
);

uuid_id!(
    /// Unique identifier for a device session.
    DeviceSessionId
);

/// The kind of entity that owns a namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum NamespaceKind {
    /// Namespace belongs to a user.
    User,
    /// Namespace belongs to an organization.
    Org,
}

impl NamespaceKind {
    /// Return the stable database storage value for this namespace kind.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Org => "org",
        }
    }

    /// Parse a namespace kind from its database representation.
    #[must_use]
    pub fn parse_db(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "org" => Some(Self::Org),
            _ => None,
        }
    }
}

impl fmt::Display for NamespaceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Role within an organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    /// Organization owner.
    Owner,
    /// Organization admin.
    Admin,
    /// Organization member.
    Member,
}

impl OrgRole {
    /// Return the stable database storage value for this role.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    /// Parse an org role from its database representation.
    #[must_use]
    pub fn parse_db(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Self::Owner),
            "admin" => Some(Self::Admin),
            "member" => Some(Self::Member),
            _ => None,
        }
    }

    /// Return whether this role can invite new members.
    #[must_use]
    pub fn can_invite(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    /// Return whether this role can remove a target role.
    #[must_use]
    pub fn can_remove(self, target: Self) -> bool {
        match self {
            Self::Owner => target != Self::Owner,
            Self::Admin => target == Self::Member,
            Self::Member => false,
        }
    }

    /// Return whether this role can change a target member's role.
    #[must_use]
    pub fn can_change_role(self, _target: Self) -> bool {
        self == Self::Owner
    }
}

impl fmt::Display for OrgRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    //! Tests for shared identifier and storage enums.

    use std::str::FromStr;

    use uuid::Uuid;

    use super::{DeviceSessionId, NamespaceKind, OrgRole};

    /// Proves UUID-backed IDs round-trip through strings.
    #[test]
    fn uuid_ids_parse_and_display() {
        let uuid = Uuid::from_u128(1);
        let id = DeviceSessionId::from_str(uuid.to_string().as_str()).expect("valid uuid");

        assert_eq!(id.to_string(), uuid.to_string());
    }

    /// Proves namespace kind storage values stay stable.
    #[test]
    fn namespace_kind_storage_values_are_stable() {
        assert_eq!(NamespaceKind::User.as_str(), "user");
        assert_eq!(NamespaceKind::Org.as_str(), "org");
        assert_eq!(NamespaceKind::parse_db("missing"), None);
    }

    /// Proves org role storage values and permissions stay stable.
    #[test]
    fn org_role_storage_and_permissions_are_stable() {
        assert_eq!(OrgRole::Owner.as_str(), "owner");
        assert!(OrgRole::Admin.can_invite());
        assert!(OrgRole::Owner.can_remove(OrgRole::Admin));
        assert!(!OrgRole::Admin.can_remove(OrgRole::Owner));
        assert!(OrgRole::Owner.can_change_role(OrgRole::Member));
    }
}
