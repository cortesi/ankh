//! Shared database support helpers.

use std::fmt::Write;

use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version,
    password_hash::{self, phc::PasswordHash},
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
        let hasher = Argon2::new(Algorithm::Argon2id, Version::V0x13, self.params.clone());
        let hash: PasswordHash = hasher.hash_password(password.as_bytes())?;
        Ok(hash.to_string())
    }
}

/// Verify a password against a stored PHC hash string.
///
/// The cost parameters come from the stored hash, so credentials hashed with
/// older parameters keep verifying.
pub fn verify_password(password: &str, stored_hash: &str) -> Result<()> {
    match Argon2::default().verify_password(password.as_bytes(), stored_hash) {
        Ok(()) => Ok(()),
        Err(password_hash::Error::PasswordInvalid) => Err(Error::InvalidCredentials),
        Err(err) => Err(Error::PasswordHash(err)),
    }
}

/// Fill a buffer with bytes from the operating system random source.
pub fn fill_random(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|err| Error::PasswordHash(err.into()))
}

impl Default for PasswordHashing {
    fn default() -> Self {
        Self::production()
    }
}

/// Hash a raw secret for database storage.
///
/// Secrets are stored as lowercase hex-encoded SHA-256 digests to avoid leaking
/// raw values through database access.
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

    use super::{
        ParsedCursor, PasswordHashing, hash_secret, make_cursor, verify_password,
        waitlist_status_from_db,
    };
    use crate::Error;

    /// A production-parameter hash written by argon2 0.5, as stored before the
    /// argon2 0.6 upgrade.
    const LEGACY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$zwPRMD5DcJmegppRU2PfhQ$Xhy4t6yewpN0Kcs1Rg6cpQ+WkJwcj+xFcew1n5ximxA";

    /// Proves hashes stored by earlier argon2 releases still verify.
    #[test]
    fn legacy_hash_verifies() {
        verify_password("legacy-pass", LEGACY_HASH).expect("legacy hash verifies");
        assert!(matches!(
            verify_password("wrong-pass", LEGACY_HASH),
            Err(Error::InvalidCredentials)
        ));
    }

    /// Proves a freshly hashed password verifies and rejects other passwords.
    #[test]
    fn hash_round_trip() {
        let hash = PasswordHashing::testing()
            .hash_password("fresh-pass")
            .expect("hash password");

        assert!(hash.starts_with("$argon2id$v=19$"));
        verify_password("fresh-pass", &hash).expect("fresh hash verifies");
        assert!(matches!(
            verify_password("other-pass", &hash),
            Err(Error::InvalidCredentials)
        ));
    }

    /// Proves a malformed stored hash is a hashing error, not a credential
    /// error.
    #[test]
    fn malformed_hash_is_hash_error() {
        assert!(matches!(
            verify_password("any", "not-a-phc-string"),
            Err(Error::PasswordHash(_))
        ));
    }

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
