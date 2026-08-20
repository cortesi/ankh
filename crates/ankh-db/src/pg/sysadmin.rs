//! Sysadmin operations.

use std::time::Duration;

use argon2::{
    Argon2, PasswordVerifier,
    password_hash::{Error as PasswordError, PasswordHash},
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    AnkhDb, Error, ParsedCursor, Result, SysadminId, SysadminInfo, hash_secret, make_cursor,
};

/// Creates a new sysadmin account with the given email and password.
pub async fn add_sysadmin(db: &AnkhDb, email: &str, password: &str) -> Result<SysadminId> {
    let password_hash = db.hash_password(password)?;
    let row = db
        .client
        .query_opt(
            "INSERT INTO sysadmins (email, password_hash)
         VALUES ($1, $2)
         ON CONFLICT (email) DO NOTHING
         RETURNING id",
            &[&email, &password_hash],
        )
        .await?;

    match row {
        Some(row) => {
            let id: uuid::Uuid = row.get(0);
            Ok(SysadminId(id))
        }
        None => Err(Error::SysadminExists(email.to_owned())),
    }
}

/// Authenticates a sysadmin and returns a newly created token plus sysadmin info.
pub async fn sysadmin_login(
    db: &AnkhDb,
    email: &str,
    password: &str,
    ttl: Duration,
) -> Result<(String, SysadminInfo)> {
    let row = db
        .client
        .query_opt(
            "SELECT id, password_hash, created_at, last_login_at, disabled_at
         FROM sysadmins
         WHERE email = $1",
            &[&email],
        )
        .await?;

    let row = match row {
        Some(row) => row,
        None => return Err(Error::InvalidCredentials),
    };

    let id: uuid::Uuid = row.get(0);
    let password_hash: String = row.get(1);
    let created_at: DateTime<Utc> = row.get(2);
    let disabled_at: Option<DateTime<Utc>> = row.get(4);

    if disabled_at.is_some() {
        return Err(Error::SysadminDisabled(email.to_owned()));
    }

    let parsed_hash = PasswordHash::new(&password_hash)?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => {}
        Err(PasswordError::Password) => return Err(Error::InvalidCredentials),
        Err(err) => return Err(Error::PasswordHash(err)),
    }

    // Update last_login_at
    db.client
        .execute(
            "UPDATE sysadmins SET last_login_at = CURRENT_TIMESTAMP WHERE id = $1",
            &[&id],
        )
        .await?;

    // Generate token
    let token = Uuid::new_v4().to_string();
    let token_hash = hash_secret(&token);
    let ttl_seconds: i64 = ttl.as_secs().try_into().unwrap_or(i64::MAX);

    db.client
        .execute(
            "INSERT INTO sysadmin_tokens (sysadmin_id, token_hash, expires_at)
         VALUES ($1, $2, CURRENT_TIMESTAMP + ($3::BIGINT * INTERVAL '1 second'))",
            &[&id, &token_hash, &ttl_seconds],
        )
        .await?;

    let sysadmin_info = SysadminInfo {
        id: SysadminId(id),
        email: email.to_owned(),
        created_at,
        last_login_at: Some(Utc::now()),
        disabled_at: None,
    };

    Ok((token, sysadmin_info))
}

/// Validates a sysadmin token and returns the sysadmin info.
pub async fn validate_sysadmin_token(db: &AnkhDb, token: &str) -> Result<SysadminInfo> {
    let token_hash = hash_secret(token);

    // Clean up expired tokens first
    db.client
        .execute(
            "DELETE FROM sysadmin_tokens WHERE token_hash = $1 AND expires_at <= CURRENT_TIMESTAMP",
            &[&token_hash],
        )
        .await?;

    let row = db
        .client
        .query_opt(
            "SELECT t.sysadmin_id, t.revoked_at, a.email, a.created_at, a.last_login_at, a.disabled_at
         FROM sysadmin_tokens t
         JOIN sysadmins a ON t.sysadmin_id = a.id
         WHERE t.token_hash = $1
           AND t.expires_at > CURRENT_TIMESTAMP",
            &[&token_hash],
        )
        .await?;

    let row = row.ok_or_else(|| Error::SysadminTokenNotFound(token_hash.clone()))?;

    let sysadmin_id: uuid::Uuid = row.get(0);
    let revoked_at: Option<DateTime<Utc>> = row.get(1);
    let email: String = row.get(2);
    let created_at: DateTime<Utc> = row.get(3);
    let last_login_at: Option<DateTime<Utc>> = row.get(4);
    let disabled_at: Option<DateTime<Utc>> = row.get(5);

    if revoked_at.is_some() {
        return Err(Error::SysadminTokenNotFound(token_hash));
    }

    if disabled_at.is_some() {
        return Err(Error::SysadminDisabled(email));
    }

    // Update last_used_at
    db.client
        .execute(
            "UPDATE sysadmin_tokens SET last_used_at = CURRENT_TIMESTAMP WHERE token_hash = $1",
            &[&token_hash],
        )
        .await?;

    Ok(SysadminInfo {
        id: SysadminId(sysadmin_id),
        email,
        created_at,
        last_login_at,
        disabled_at,
    })
}

/// Revokes a sysadmin token.
pub async fn revoke_sysadmin_token(db: &AnkhDb, token: &str) -> Result<()> {
    let token_hash = hash_secret(token);
    let updated = db
        .client
        .execute(
            "UPDATE sysadmin_tokens SET revoked_at = CURRENT_TIMESTAMP
         WHERE token_hash = $1 AND revoked_at IS NULL",
            &[&token_hash],
        )
        .await?;

    if updated == 0 {
        return Err(Error::SysadminTokenNotFound(token_hash));
    }

    Ok(())
}

/// Lists sysadmin accounts with cursor-based pagination.
pub async fn list_sysadmins(
    db: &AnkhDb,
    limit: i64,
    cursor: Option<&str>,
) -> Result<(Vec<SysadminInfo>, Option<String>)> {
    let parsed = cursor
        .map(|c| {
            ParsedCursor::parse(c)
                .ok_or_else(|| Error::SysadminMissing(format!("invalid cursor: {c}")))
        })
        .transpose()?;

    // Fetch one extra row to learn whether a further page exists without
    // emitting a cursor that points at an empty page.
    let fetch_limit = limit + 1;

    let rows = if let Some(c) = &parsed {
        db.client
            .query(
                "SELECT id, email, created_at, last_login_at, disabled_at
             FROM sysadmins WHERE (created_at, id) < ($1, $2)
             ORDER BY created_at DESC, id DESC LIMIT $3",
                &[&c.time, &c.id, &fetch_limit],
            )
            .await?
    } else {
        db.client
            .query(
                "SELECT id, email, created_at, last_login_at, disabled_at
             FROM sysadmins ORDER BY created_at DESC, id DESC LIMIT $1",
                &[&fetch_limit],
            )
            .await?
    };

    let has_more = rows.len() as i64 > limit;
    let sysadmins: Vec<SysadminInfo> = rows
        .iter()
        .take(limit as usize)
        .map(|row| {
            let id: uuid::Uuid = row.get(0);
            SysadminInfo {
                id: SysadminId(id),
                email: row.get(1),
                created_at: row.get(2),
                last_login_at: row.get(3),
                disabled_at: row.get(4),
            }
        })
        .collect();

    let next_cursor = has_more
        .then(|| {
            sysadmins
                .last()
                .map(|a| make_cursor(&a.created_at, &a.id.0))
        })
        .flatten();

    Ok((sysadmins, next_cursor))
}

/// Gets a sysadmin by ID.
pub async fn get_sysadmin(db: &AnkhDb, id: SysadminId) -> Result<SysadminInfo> {
    let row = db
        .client
        .query_opt(
            "SELECT id, email, created_at, last_login_at, disabled_at
         FROM sysadmins
         WHERE id = $1",
            &[&id.0],
        )
        .await?;

    let row = row.ok_or_else(|| Error::SysadminMissing(id.to_string()))?;

    Ok(SysadminInfo {
        id: SysadminId(row.get(0)),
        email: row.get(1),
        created_at: row.get(2),
        last_login_at: row.get(3),
        disabled_at: row.get(4),
    })
}

/// Deletes expired and revoked sysadmin tokens.
pub async fn delete_expired_sysadmin_tokens(db: &AnkhDb) -> Result<u64> {
    let deleted = db
        .client
        .execute(
            "DELETE FROM sysadmin_tokens
         WHERE expires_at <= CURRENT_TIMESTAMP
            OR revoked_at IS NOT NULL",
            &[],
        )
        .await?;
    Ok(deleted)
}
