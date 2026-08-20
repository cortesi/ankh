//! Device authorization and session operations.

use std::time::Duration;

use ankh_constants::{DEVICE_AUTH_GRANT_MAX_ATTEMPTS, DEVICE_SESSION_LIMIT};
use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::{
    AnkhDb, CreatedDeviceAuthGrant, CreatedDeviceSession, DeviceAuthGrant, DeviceAuthGrantId,
    DeviceAuthGrantRequest, DevicePlatform, DeviceSession, DeviceSessionId, DeviceSessionStatus,
    DeviceSessionSummary, Error, Result, UserId, hash_secret, make_cursor,
};

/// Number of random bytes used for auth codes and session tokens.
const TOKEN_BYTES: usize = 32;

/// Generate a base64url-encoded random string.
fn random_base64_url(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Compute the PKCE S256 challenge for the supplied verifier.
fn pkce_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    URL_SAFE_NO_PAD.encode(digest)
}

/// Maximum allowed verification attempts as stored in the database.
fn max_grant_attempts() -> i32 {
    i32::try_from(DEVICE_AUTH_GRANT_MAX_ATTEMPTS).unwrap_or(i32::MAX)
}

/// Build a device auth grant from a database row.
fn device_grant_from_row(row: &tokio_postgres::Row) -> DeviceAuthGrant {
    let platform: String = row.get(6);
    DeviceAuthGrant {
        id: DeviceAuthGrantId(row.get(0)),
        user_id: UserId(row.get(1)),
        code_challenge: row.get(2),
        state: row.get(3),
        redirect_port: row.get(4),
        device_name: row.get(5),
        platform: DevicePlatform::parse_db(platform.as_str()),
        attempts: row.get(7),
        created_at: row.get(8),
        expires_at: row.get(9),
        consumed_at: row.get(10),
    }
}

/// Build a device session from a database row.
fn device_session_from_row(row: &tokio_postgres::Row) -> DeviceSession {
    let platform: String = row.get(3);
    DeviceSession {
        id: DeviceSessionId(row.get(0)),
        user_id: UserId(row.get(1)),
        device_name: row.get(2),
        platform: DevicePlatform::parse_db(platform.as_str()),
        created_at: row.get(4),
        last_used_at: row.get(5),
        expires_at: row.get(6),
        revoked_at: row.get(7),
    }
}

/// Create a device auth grant for a user.
pub async fn create_device_auth_grant(
    db: &AnkhDb,
    request: DeviceAuthGrantRequest<'_>,
) -> Result<CreatedDeviceAuthGrant> {
    delete_expired_device_auth_grants(db).await?;

    let code = random_base64_url(TOKEN_BYTES);
    let code_hash = hash_secret(code.as_str());
    let ttl_seconds: i64 = request.ttl.as_secs().try_into().unwrap_or(i64::MAX);

    let row = db
        .client
        .query_opt(
            "INSERT INTO device_auth_grants
                (user_id, code_hash, code_challenge, state, redirect_port, device_name, platform, expires_at)
             SELECT $1, $2, $3, $4, $5, $6, $7,
                CURRENT_TIMESTAMP + ($8::BIGINT * INTERVAL '1 second')
             WHERE EXISTS (SELECT 1 FROM users WHERE id = $1)
             RETURNING id",
            &[
                &request.user_id.0,
                &code_hash,
                &request.code_challenge,
                &request.state,
                &request.redirect_port,
                &request.device_name,
                &request.platform.as_str(),
                &ttl_seconds,
            ],
        )
        .await?;

    let row = row.ok_or_else(|| Error::UserMissing(request.user_id.to_string()))?;
    let id: uuid::Uuid = row.get(0);

    Ok(CreatedDeviceAuthGrant {
        id: DeviceAuthGrantId(id),
        code,
    })
}

/// Consume a device auth grant and validate the PKCE verifier.
pub async fn consume_device_auth_grant(
    db: &mut AnkhDb,
    code: &str,
    code_verifier: &str,
) -> Result<DeviceAuthGrant> {
    let code_hash = hash_secret(code);
    let tx = db.client.transaction().await?;

    let row = tx
        .query_opt(
            "SELECT id, user_id, code_challenge, state, redirect_port, device_name, platform,
                    attempts, created_at, expires_at, consumed_at
             FROM device_auth_grants
             WHERE code_hash = $1
             FOR UPDATE",
            &[&code_hash],
        )
        .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Err(Error::DeviceAuthGrantMissing(code_hash));
    };

    let expires_at: DateTime<Utc> = row.get(9);
    let consumed_at: Option<DateTime<Utc>> = row.get(10);
    let attempts: i32 = row.get(7);

    if consumed_at.is_some() {
        tx.commit().await?;
        return Err(Error::DeviceAuthGrantConsumed(code_hash));
    }

    if expires_at <= Utc::now() {
        tx.commit().await?;
        return Err(Error::DeviceAuthGrantExpired(code_hash));
    }

    if attempts >= max_grant_attempts() {
        tx.execute(
            "UPDATE device_auth_grants SET consumed_at = CURRENT_TIMESTAMP WHERE id = $1",
            &[&row.get::<_, uuid::Uuid>(0)],
        )
        .await?;
        tx.commit().await?;
        return Err(Error::DeviceAuthGrantAttemptsExceeded(code_hash));
    }

    let expected = row.get::<_, String>(2);
    let actual = pkce_challenge(code_verifier);
    if actual != expected {
        let next_attempts = attempts.saturating_add(1);
        let reached_limit = next_attempts >= max_grant_attempts();
        tx.execute(
            "UPDATE device_auth_grants
             SET attempts = $2,
                 consumed_at = CASE WHEN $3 THEN CURRENT_TIMESTAMP ELSE consumed_at END
             WHERE id = $1",
            &[&row.get::<_, uuid::Uuid>(0), &next_attempts, &reached_limit],
        )
        .await?;
        tx.commit().await?;
        return Err(Error::DeviceAuthGrantInvalidVerifier(code_hash));
    }

    tx.execute(
        "UPDATE device_auth_grants SET consumed_at = CURRENT_TIMESTAMP WHERE id = $1",
        &[&row.get::<_, uuid::Uuid>(0)],
    )
    .await?;

    tx.commit().await?;
    let mut grant = device_grant_from_row(&row);
    grant.consumed_at = Some(Utc::now());
    Ok(grant)
}

/// Create a device session for the supplied user.
pub async fn create_device_session(
    db: &AnkhDb,
    user_id: UserId,
    device_name: &str,
    platform: &DevicePlatform,
    ttl: Duration,
) -> Result<CreatedDeviceSession> {
    delete_expired_device_sessions(db).await?;

    let user_exists = db
        .client
        .query_opt("SELECT 1 FROM users WHERE id = $1", &[&user_id.0])
        .await?
        .is_some();
    if !user_exists {
        return Err(Error::UserMissing(user_id.to_string()));
    }

    let active_count: i64 = db
        .client
        .query_one(
            "SELECT COUNT(*)
             FROM device_sessions
             WHERE user_id = $1
               AND revoked_at IS NULL
               AND expires_at > CURRENT_TIMESTAMP",
            &[&user_id.0],
        )
        .await?
        .get(0);

    if active_count >= i64::from(DEVICE_SESSION_LIMIT) {
        return Err(Error::DeviceSessionLimitReached(user_id.to_string()));
    }

    let token = random_base64_url(TOKEN_BYTES);
    let token_hash = hash_secret(token.as_str());
    let ttl_seconds: i64 = ttl.as_secs().try_into().unwrap_or(i64::MAX);

    let row = db
        .client
        .query_one(
            "INSERT INTO device_sessions
                (user_id, token_hash, device_name, platform, expires_at)
             VALUES ($1, $2, $3, $4,
                CURRENT_TIMESTAMP + ($5::BIGINT * INTERVAL '1 second'))
             RETURNING id, user_id, device_name, platform, created_at, last_used_at, expires_at, revoked_at",
            &[
                &user_id.0,
                &token_hash,
                &device_name,
                &platform.as_str(),
                &ttl_seconds,
            ],
        )
        .await?;

    let session = device_session_from_row(&row);
    Ok(CreatedDeviceSession { session, token })
}

/// Validate a device session token and return session metadata.
pub async fn validate_device_session(db: &AnkhDb, token: &str) -> Result<DeviceSession> {
    let token_hash = hash_secret(token);
    let row = db
        .client
        .query_opt(
            "SELECT id, user_id, device_name, platform, created_at, last_used_at, expires_at, revoked_at
             FROM device_sessions
             WHERE token_hash = $1",
            &[&token_hash],
        )
        .await?;

    let row = row.ok_or_else(|| Error::DeviceSessionMissing(token_hash.clone()))?;
    let expires_at: DateTime<Utc> = row.get(6);
    let revoked_at: Option<DateTime<Utc>> = row.get(7);

    if revoked_at.is_some() {
        return Err(Error::DeviceSessionRevoked(token_hash));
    }
    if expires_at <= Utc::now() {
        return Err(Error::DeviceSessionExpired(token_hash));
    }

    db.client
        .execute(
            "UPDATE device_sessions SET last_used_at = CURRENT_TIMESTAMP WHERE id = $1",
            &[&row.get::<_, uuid::Uuid>(0)],
        )
        .await?;

    Ok(device_session_from_row(&row))
}

/// List active device sessions for a given user.
pub async fn list_device_sessions_for_user(
    db: &AnkhDb,
    user_id: UserId,
) -> Result<Vec<DeviceSession>> {
    let rows = db
        .client
        .query(
            "SELECT id, user_id, device_name, platform, created_at, last_used_at, expires_at, revoked_at
             FROM device_sessions
             WHERE user_id = $1
               AND revoked_at IS NULL
               AND expires_at > CURRENT_TIMESTAMP
             ORDER BY created_at DESC, id DESC",
            &[&user_id.0],
        )
        .await?;

    Ok(rows.iter().map(device_session_from_row).collect())
}

/// Get a device session by ID.
pub async fn get_device_session(db: &AnkhDb, id: DeviceSessionId) -> Result<DeviceSession> {
    let row = db
        .client
        .query_opt(
            "SELECT id, user_id, device_name, platform, created_at, last_used_at, expires_at, revoked_at
             FROM device_sessions
             WHERE id = $1",
            &[&id.0],
        )
        .await?;

    row.as_ref()
        .map(device_session_from_row)
        .ok_or_else(|| Error::DeviceSessionMissing(id.to_string()))
}

/// Revoke a device session belonging to the supplied user.
pub async fn revoke_device_session(
    db: &AnkhDb,
    id: DeviceSessionId,
    user_id: UserId,
) -> Result<()> {
    let updated = db
        .client
        .execute(
            "UPDATE device_sessions
             SET revoked_at = CURRENT_TIMESTAMP
             WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
            &[&id.0, &user_id.0],
        )
        .await?;

    if updated == 0 {
        return Err(Error::DeviceSessionMissing(id.to_string()));
    }

    Ok(())
}

/// Revoke all device sessions for the supplied user.
pub async fn revoke_all_device_sessions(db: &AnkhDb, user_id: UserId) -> Result<u64> {
    let updated = db
        .client
        .execute(
            "UPDATE device_sessions
             SET revoked_at = CURRENT_TIMESTAMP
             WHERE user_id = $1 AND revoked_at IS NULL",
            &[&user_id.0],
        )
        .await?;
    Ok(updated)
}

/// List device sessions with cursor-based pagination.
pub async fn list_device_sessions(
    db: &AnkhDb,
    limit: i64,
    cursor: Option<&str>,
    user_id: Option<UserId>,
    status: Option<DeviceSessionStatus>,
) -> Result<(Vec<DeviceSessionSummary>, Option<String>)> {
    let parsed = cursor
        .map(|c| {
            crate::ParsedCursor::parse(c)
                .ok_or_else(|| Error::DeviceSessionMissing(format!("invalid cursor: {c}")))
        })
        .transpose()?;

    let base = "SELECT s.id, u.id, u.email, s.device_name, s.platform,
                       s.created_at, s.last_used_at, s.expires_at, s.revoked_at
                FROM device_sessions s JOIN users u ON s.user_id = u.id";
    let status_cond = match status {
        Some(DeviceSessionStatus::Active) => {
            " AND s.revoked_at IS NULL AND s.expires_at > CURRENT_TIMESTAMP"
        }
        Some(DeviceSessionStatus::Revoked) => " AND s.revoked_at IS NOT NULL",
        Some(DeviceSessionStatus::Expired) => {
            " AND s.revoked_at IS NULL AND s.expires_at <= CURRENT_TIMESTAMP"
        }
        None => "",
    };
    let order = " ORDER BY s.created_at DESC, s.id DESC";

    // Fetch one extra row to learn whether a further page exists without
    // emitting a cursor that points at an empty page.
    let fetch_limit = limit + 1;

    let rows = match (&parsed, &user_id) {
        (Some(c), Some(uid)) => {
            let q = format!(
                "{base} WHERE (s.created_at, s.id) < ($1, $2) AND u.id = $3{status_cond}{order} LIMIT $4"
            );
            db.client
                .query(&q, &[&c.time, &c.id, &uid.0, &fetch_limit])
                .await?
        }
        (Some(c), None) => {
            let q = format!(
                "{base} WHERE (s.created_at, s.id) < ($1, $2){status_cond}{order} LIMIT $3"
            );
            db.client.query(&q, &[&c.time, &c.id, &fetch_limit]).await?
        }
        (None, Some(uid)) => {
            let q = format!("{base} WHERE u.id = $1{status_cond}{order} LIMIT $2");
            db.client.query(&q, &[&uid.0, &fetch_limit]).await?
        }
        (None, None) if status_cond.is_empty() => {
            let q = format!("{base}{order} LIMIT $1");
            db.client.query(&q, &[&fetch_limit]).await?
        }
        (None, None) => {
            let q = format!("{base} WHERE 1=1{status_cond}{order} LIMIT $1");
            db.client.query(&q, &[&fetch_limit]).await?
        }
    };

    let has_more = rows.len() as i64 > limit;
    let sessions: Vec<DeviceSessionSummary> = rows
        .iter()
        .take(limit as usize)
        .map(|row| {
            let platform: String = row.get(4);
            Ok(DeviceSessionSummary {
                id: DeviceSessionId(row.get(0)),
                user_id: UserId(row.get(1)),
                user_email: row.get(2),
                device_name: row.get(3),
                platform: DevicePlatform::parse_db(platform.as_str()),
                created_at: row.get(5),
                last_used_at: row.get(6),
                expires_at: row.get(7),
                revoked_at: row.get(8),
            })
        })
        .collect::<Result<_>>()?;

    let next_cursor = has_more
        .then(|| sessions.last().map(|s| make_cursor(&s.created_at, &s.id.0)))
        .flatten();

    Ok((sessions, next_cursor))
}

/// Revoke a device session by ID.
pub async fn revoke_device_session_by_id(db: &AnkhDb, id: DeviceSessionId) -> Result<()> {
    let updated = db
        .client
        .execute(
            "UPDATE device_sessions SET revoked_at = CURRENT_TIMESTAMP
             WHERE id = $1 AND revoked_at IS NULL",
            &[&id.0],
        )
        .await?;

    if updated == 0 {
        return Err(Error::DeviceSessionMissing(id.to_string()));
    }

    Ok(())
}

/// Remove expired or consumed device auth grants.
async fn delete_expired_device_auth_grants(db: &AnkhDb) -> Result<u64> {
    let deleted = db
        .client
        .execute(
            "DELETE FROM device_auth_grants
             WHERE expires_at <= CURRENT_TIMESTAMP OR consumed_at IS NOT NULL",
            &[],
        )
        .await?;
    Ok(deleted)
}

/// Remove expired device sessions.
async fn delete_expired_device_sessions(db: &AnkhDb) -> Result<u64> {
    let deleted = db
        .client
        .execute(
            "DELETE FROM device_sessions WHERE expires_at <= CURRENT_TIMESTAMP",
            &[],
        )
        .await?;
    Ok(deleted)
}
