//! Session operations.

use std::time::Duration;

use argon2::{
    Argon2, PasswordVerifier,
    password_hash::{Error as PasswordError, PasswordHash},
};
use uuid::Uuid;

use crate::{
    AnkhDb, Error, ParsedCursor, Result, Session, SessionId, SessionStatus, SessionSummary, UserId,
    hash_secret, make_cursor,
};

/// Authenticates a user and returns a newly created session identifier.
///
/// The `identifier` can be either an email address or a username. If the
/// identifier contains `@`, it's treated as an email; otherwise as a username.
pub async fn signin(
    db: &AnkhDb,
    identifier: &str,
    password: &str,
    ttl: Duration,
) -> Result<String> {
    // Determine if identifier is email or username
    let (email, password_hash): (String, String) = if identifier.contains('@') {
        // Treat as email
        let row = db
            .client
            .query_opt(
                "SELECT email, password_hash FROM users WHERE email = $1",
                &[&identifier],
            )
            .await?;
        match row {
            Some(row) => (row.get(0), row.get(1)),
            None => return Err(Error::InvalidCredentials),
        }
    } else {
        // Treat as username - look up via namespace
        let normalized = identifier.to_lowercase();
        let row = db
            .client
            .query_opt(
                "SELECT u.email, u.password_hash
             FROM users u
             JOIN namespaces n ON n.id = u.namespace_id
             WHERE n.name = $1",
                &[&normalized],
            )
            .await?;
        match row {
            Some(row) => (row.get(0), row.get(1)),
            None => return Err(Error::InvalidCredentials),
        }
    };

    let parsed_hash = PasswordHash::new(&password_hash)?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => {}
        Err(PasswordError::Password) => return Err(Error::InvalidCredentials),
        Err(err) => return Err(Error::PasswordHash(err)),
    }

    let session_id = Uuid::new_v4().to_string();
    delete_expired_sessions(db).await?;
    db.insert_session_for_user(&session_id, &email, ttl).await?;

    Ok(session_id)
}

/// Adds a new session with a caller-provided identifier for an existing user.
pub async fn add_session(db: &AnkhDb, session_id: &str, email: &str, ttl: Duration) -> Result<()> {
    db.insert_session_for_user(session_id, email, ttl).await
}

/// Fetches session details without modifying the stored touch timestamp.
pub async fn get_session(db: &AnkhDb, session_id: &str) -> Result<Session> {
    let session_hash = hash_secret(session_id);
    db.client
        .execute(
            "DELETE FROM sessions WHERE token_hash = $1 AND expires_at <= CURRENT_TIMESTAMP",
            &[&session_hash],
        )
        .await?;
    let row = db
        .client
        .query_opt(
            "SELECT email, created_at, touched_at, expires_at
         FROM sessions
         WHERE token_hash = $1
           AND expires_at > CURRENT_TIMESTAMP",
            &[&session_hash],
        )
        .await?;

    let row = row.ok_or_else(|| Error::SessionMissing(session_hash))?;

    Ok(AnkhDb::session_from_row(&row))
}

/// Refreshes the `touched_at` timestamp for a session and returns session details.
pub async fn touch_session(db: &AnkhDb, session_id: &str) -> Result<Session> {
    let session_hash = hash_secret(session_id);
    db.client
        .execute(
            "DELETE FROM sessions WHERE token_hash = $1 AND expires_at <= CURRENT_TIMESTAMP",
            &[&session_hash],
        )
        .await?;
    let row = db
        .client
        .query_opt(
            "UPDATE sessions
         SET touched_at = CURRENT_TIMESTAMP
         WHERE token_hash = $1
           AND expires_at > CURRENT_TIMESTAMP
         RETURNING email, created_at, touched_at, expires_at",
            &[&session_hash],
        )
        .await?;

    let row = row.ok_or_else(|| Error::SessionMissing(session_hash))?;

    Ok(AnkhDb::session_from_row(&row))
}

/// Touches a session only if the last touch timestamp is older than the supplied threshold.
pub async fn touch_session_if_stale(
    db: &mut AnkhDb,
    session_id: &str,
    stale_after: Duration,
) -> Result<Session> {
    let session_hash = hash_secret(session_id);
    let tx = db.client.transaction().await?;

    tx.execute(
        "DELETE FROM sessions WHERE token_hash = $1 AND expires_at <= CURRENT_TIMESTAMP",
        &[&session_hash],
    )
    .await?;

    if stale_after.is_zero() {
        let row = tx
            .query_opt(
                "UPDATE sessions
             SET touched_at = CURRENT_TIMESTAMP
             WHERE token_hash = $1
               AND expires_at > CURRENT_TIMESTAMP
             RETURNING email, created_at, touched_at, expires_at",
                &[&session_hash],
            )
            .await?;

        let row = row.ok_or_else(|| Error::SessionMissing(session_hash))?;
        tx.commit().await?;
        return Ok(AnkhDb::session_from_row(&row));
    }

    let stale_seconds: i64 = stale_after.as_secs().try_into().unwrap_or(i64::MAX);

    let row = tx
        .query_opt(
            "WITH updated AS (
             UPDATE sessions
             SET touched_at = CURRENT_TIMESTAMP
             WHERE token_hash = $1
               AND expires_at > CURRENT_TIMESTAMP
               AND touched_at < (CURRENT_TIMESTAMP - ($2::BIGINT * INTERVAL '1 second'))
             RETURNING email, created_at, touched_at, expires_at
         )
         SELECT email, created_at, touched_at, expires_at
         FROM updated
         UNION ALL
         SELECT email, created_at, touched_at, expires_at
         FROM sessions
         WHERE token_hash = $1
           AND expires_at > CURRENT_TIMESTAMP
           AND NOT EXISTS (SELECT 1 FROM updated)",
            &[&session_hash, &stale_seconds],
        )
        .await?;

    let row = row.ok_or_else(|| Error::SessionMissing(session_hash))?;
    tx.commit().await?;

    Ok(AnkhDb::session_from_row(&row))
}

/// Deletes a session by identifier.
pub async fn delete_session(db: &AnkhDb, session_id: &str) -> Result<()> {
    let session_hash = hash_secret(session_id);
    let deleted = db
        .client
        .execute(
            "DELETE FROM sessions WHERE token_hash = $1",
            &[&session_hash],
        )
        .await?;

    if deleted == 0 {
        return Err(Error::SessionMissing(session_hash));
    }

    Ok(())
}

/// Delete all sessions for a given user email address.
pub async fn delete_sessions_for_email(db: &AnkhDb, email: &str) -> Result<u64> {
    let deleted = db
        .client
        .execute("DELETE FROM sessions WHERE email = $1", &[&email])
        .await?;
    Ok(deleted)
}

/// Delete all sessions that have expired.
pub async fn delete_expired_sessions(db: &AnkhDb) -> Result<u64> {
    let deleted = db
        .client
        .execute(
            "DELETE FROM sessions WHERE expires_at <= CURRENT_TIMESTAMP",
            &[],
        )
        .await?;
    Ok(deleted)
}

/// Lists sessions with cursor-based pagination.
pub async fn list_sessions(
    db: &AnkhDb,
    limit: i64,
    cursor: Option<&str>,
    user_id: Option<UserId>,
    status: Option<SessionStatus>,
) -> Result<(Vec<SessionSummary>, Option<String>)> {
    let parsed = cursor
        .map(|c| {
            ParsedCursor::parse(c)
                .ok_or_else(|| Error::SessionMissing(format!("invalid cursor: {c}")))
        })
        .transpose()?;

    let base = "SELECT s.id, u.id, u.email, s.created_at, s.touched_at, s.expires_at, s.revoked_at
                FROM sessions s JOIN users u ON s.email = u.email";
    let status_cond = match status {
        Some(SessionStatus::Active) => {
            " AND s.revoked_at IS NULL AND s.expires_at > CURRENT_TIMESTAMP"
        }
        Some(SessionStatus::Revoked) => " AND s.revoked_at IS NOT NULL",
        Some(SessionStatus::Expired) => {
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
    let sessions: Vec<SessionSummary> = rows
        .iter()
        .take(limit as usize)
        .map(|row| SessionSummary {
            id: SessionId(row.get(0)),
            user_id: UserId(row.get(1)),
            user_email: row.get(2),
            created_at: row.get(3),
            touched_at: row.get(4),
            expires_at: row.get(5),
            revoked_at: row.get(6),
        })
        .collect();

    let next_cursor = has_more
        .then(|| sessions.last().map(|s| make_cursor(&s.created_at, &s.id.0)))
        .flatten();

    Ok((sessions, next_cursor))
}

/// Revokes a session by ID.
pub async fn revoke_session_by_id(db: &AnkhDb, id: SessionId) -> Result<()> {
    let updated = db
        .client
        .execute(
            "UPDATE sessions SET revoked_at = CURRENT_TIMESTAMP
         WHERE id = $1 AND revoked_at IS NULL",
            &[&id.0],
        )
        .await?;

    if updated == 0 {
        return Err(Error::SessionMissing(id.to_string()));
    }

    Ok(())
}
