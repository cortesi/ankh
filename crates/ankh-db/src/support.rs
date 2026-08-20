//! Shared database support helpers.

use std::fmt::Write;

use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Database value for an active user.
const WAITLIST_STATUS_ACTIVE: &str = "active";
/// Database value for a waitlisted user.
const WAITLIST_STATUS_WAITLISTED: &str = "waitlisted";

/// Parsed pagination cursor for list queries.
pub struct ParsedCursor {
    /// Timestamp component of the cursor.
    pub time: DateTime<Utc>,
    /// UUID component of the cursor.
    pub id: uuid::Uuid,
}

impl ParsedCursor {
    /// Parse a cursor string in the format "{created_at_iso}_{uuid}".
    #[must_use]
    pub fn parse(cursor: &str) -> Option<Self> {
        let (time, id) = cursor.split_once('_')?;
        let time = DateTime::parse_from_rfc3339(time).ok()?.with_timezone(&Utc);
        let id = uuid::Uuid::parse_str(id).ok()?;
        Some(Self { time, id })
    }
}

/// Generate a cursor string from timestamp and ID.
#[must_use]
pub fn make_cursor(time: &DateTime<Utc>, id: &uuid::Uuid) -> String {
    format!(
        "{}_{}",
        time.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        id
    )
}

/// Convert a database waitlist status string into a boolean.
pub fn waitlist_status_from_db(value: &str) -> Result<bool> {
    match value {
        WAITLIST_STATUS_ACTIVE => Ok(false),
        WAITLIST_STATUS_WAITLISTED => Ok(true),
        other => Err(Error::InvalidWaitlistStatus(other.to_owned())),
    }
}

/// Convert a waitlisted flag into the database string representation.
#[must_use]
pub fn waitlist_status_to_db(waitlisted: bool) -> &'static str {
    if waitlisted {
        WAITLIST_STATUS_WAITLISTED
    } else {
        WAITLIST_STATUS_ACTIVE
    }
}

/// Password hashing configuration used when storing new credentials.
#[derive(Clone)]
pub struct PasswordHashing {
    /// Argon2 parameters controlling hashing cost.
    params: Params,
}

impl PasswordHashing {
    /// Use production-strength Argon2 parameters.
    #[must_use]
    pub fn production() -> Self {
        Self {
            params: Params::DEFAULT,
        }
    }

    /// Use minimal Argon2 parameters suitable for unit tests.
    #[must_use]
    pub fn testing() -> Self {
        let params = Params::new(
            Params::MIN_M_COST,
            Params::MIN_T_COST,
            Params::MIN_P_COST,
            None,
        )
        .expect("argon2 test params must be valid");
        Self { params }
    }

    /// Hash a password using the configured parameters and a random salt.
    pub fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hasher = Argon2::new(Algorithm::Argon2id, Version::V0x13, self.params.clone());
        Ok(hasher
            .hash_password(password.as_bytes(), &salt)?
            .to_string())
    }
}

impl Default for PasswordHashing {
    fn default() -> Self {
        Self::production()
    }
}

/// Hash a raw secret for database storage.
///
/// Secrets are stored as lowercase hex-encoded SHA-256 digests to avoid leaking raw values through
/// database access.
#[must_use]
pub fn hash_secret(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    let digest = hasher.finalize();

    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut out, "{byte:02x}").expect("writing to string cannot fail");
    }
    out
}

#[cfg(test)]
mod tests {
    //! Tests for shared database support helpers.

    use super::{ParsedCursor, hash_secret, make_cursor, waitlist_status_from_db};

    /// Proves secret hashing is deterministic and hex encoded.
    #[test]
    fn hash_secret_is_stable_hex() {
        let hash = hash_secret("secret");

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_eq!(hash, hash_secret("secret"));
    }

    /// Proves cursor strings round-trip.
    #[test]
    fn cursor_round_trips() {
        let time = chrono::DateTime::parse_from_rfc3339("2026-06-18T01:02:03.123456Z")
            .expect("valid time")
            .with_timezone(&chrono::Utc);
        let id = uuid::Uuid::from_u128(1);
        let cursor = make_cursor(&time, &id);
        let parsed = ParsedCursor::parse(cursor.as_str()).expect("parse cursor");

        assert_eq!(parsed.time, time);
        assert_eq!(parsed.id, id);
    }

    /// Proves waitlist storage values parse.
    #[test]
    fn waitlist_status_parses() {
        assert!(!waitlist_status_from_db("active").expect("active"));
        assert!(waitlist_status_from_db("waitlisted").expect("waitlisted"));
        assert!(waitlist_status_from_db("missing").is_err());
    }
}
