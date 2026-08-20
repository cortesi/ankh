//! Invite operations.

use std::time::Duration;

use uuid::Uuid;

use crate::{AnkhDb, Error, Result, hash_secret};

/// Create a single-use invite token for the supplied email.
pub async fn create_invite(db: &AnkhDb, email: &str, ttl: Duration) -> Result<String> {
    let token = Uuid::new_v4().to_string();
    let token_hash = hash_secret(token.as_str());
    let ttl_seconds: i64 = ttl.as_secs().try_into().unwrap_or(i64::MAX);
    db.client
        .execute(
            "INSERT INTO invites (token_hash, email, expires_at)
         VALUES ($1, $2, CURRENT_TIMESTAMP + ($3::BIGINT * INTERVAL '1 second'))",
            &[&token_hash, &email, &ttl_seconds],
        )
        .await?;
    Ok(token)
}

/// Consume an invite token, returning the associated email address.
pub async fn consume_invite(db: &AnkhDb, token: &str) -> Result<String> {
    let token_hash = hash_secret(token);
    let row = db
        .client
        .query_opt(
            "DELETE FROM invites
         WHERE token_hash = $1
         RETURNING email, expires_at <= CURRENT_TIMESTAMP AS expired",
            &[&token_hash],
        )
        .await?;

    let row = row.ok_or_else(|| Error::InviteNotFound(token_hash.clone()))?;
    let email: String = row.get(0);
    let expired: bool = row.get(1);

    if expired {
        return Err(Error::InviteExpired(token_hash));
    }

    Ok(email)
}

/// Validate an invite token without consuming it, returning the associated email address.
pub async fn peek_invite(db: &AnkhDb, token: &str) -> Result<String> {
    let token_hash = hash_secret(token);
    let row = db
        .client
        .query_opt(
            "SELECT email, expires_at <= CURRENT_TIMESTAMP AS expired
         FROM invites
         WHERE token_hash = $1",
            &[&token_hash],
        )
        .await?;

    let row = row.ok_or_else(|| Error::InviteNotFound(token_hash.clone()))?;
    let email: String = row.get(0);
    let expired: bool = row.get(1);

    if expired {
        return Err(Error::InviteExpired(token_hash));
    }

    Ok(email)
}

/// Delete all invite tokens for an email.
pub async fn delete_invites_for_email(db: &AnkhDb, email: &str) -> Result<()> {
    db.client
        .execute("DELETE FROM invites WHERE email = $1", &[&email])
        .await?;
    Ok(())
}
