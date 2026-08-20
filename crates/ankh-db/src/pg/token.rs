//! Token operations.

use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{AnkhDb, Error, Result, TokenKind, hash_secret};

/// Create a single-use token for the supplied user email.
pub async fn create_token(
    db: &AnkhDb,
    email: &str,
    kind: TokenKind,
    ttl: Duration,
) -> Result<String> {
    let token = Uuid::new_v4().to_string();
    let token_hash = hash_secret(token.as_str());
    let ttl_seconds: i64 = ttl.as_secs().try_into().unwrap_or(i64::MAX);

    let inserted = db
        .client
        .execute(
            "INSERT INTO tokens (token_hash, email, kind, expires_at)
         SELECT $1, $2, $3, CURRENT_TIMESTAMP + ($4::BIGINT * INTERVAL '1 second')
         WHERE EXISTS (SELECT 1 FROM users WHERE email = $2)",
            &[&token_hash, &email, &kind.as_str(), &ttl_seconds],
        )
        .await?;

    if inserted == 0 {
        return Err(Error::UserMissing(email.to_owned()));
    }

    Ok(token)
}

/// Consume a token, returning the associated email address.
pub async fn consume_token(db: &mut AnkhDb, token: &str, kind: TokenKind) -> Result<String> {
    let token_hash = hash_secret(token);
    let kind = kind.as_str();

    let tx = db.client.transaction().await?;
    let row = tx
        .query_opt(
            "DELETE FROM tokens
         WHERE token_hash = $1
           AND kind = $2
         RETURNING email, expires_at <= CURRENT_TIMESTAMP AS expired",
            &[&token_hash, &kind],
        )
        .await?;

    let row = row.ok_or_else(|| Error::TokenNotFound(token_hash.clone()))?;
    let email: String = row.get(0);
    let expired: bool = row.get(1);

    tx.commit().await?;

    if expired {
        return Err(Error::TokenExpired(token_hash));
    }

    Ok(email)
}

/// Validate a token without consuming it, returning the associated email address.
pub async fn peek_token(db: &AnkhDb, token: &str, kind: TokenKind) -> Result<String> {
    let token_hash = hash_secret(token);
    let kind = kind.as_str();

    let row = db
        .client
        .query_opt(
            "SELECT email, expires_at <= CURRENT_TIMESTAMP AS expired
         FROM tokens
         WHERE token_hash = $1
           AND kind = $2",
            &[&token_hash, &kind],
        )
        .await?;

    let row = row.ok_or_else(|| Error::TokenNotFound(token_hash.clone()))?;
    let email: String = row.get(0);
    let expired: bool = row.get(1);

    if expired {
        return Err(Error::TokenExpired(token_hash));
    }

    Ok(email)
}

/// Return the most recent token creation time for an email and kind.
pub async fn latest_token_created_at(
    db: &AnkhDb,
    email: &str,
    kind: TokenKind,
) -> Result<Option<DateTime<Utc>>> {
    let kind = kind.as_str();
    let row = db
        .client
        .query_opt(
            "SELECT created_at
         FROM tokens
         WHERE email = $1
           AND kind = $2
         ORDER BY created_at DESC
         LIMIT 1",
            &[&email, &kind],
        )
        .await?;

    Ok(row.map(|row| row.get(0)))
}

/// Delete all tokens for an email and kind.
pub async fn delete_tokens_for_email(db: &AnkhDb, email: &str, kind: TokenKind) -> Result<()> {
    let kind = kind.as_str();
    db.client
        .execute(
            "DELETE FROM tokens WHERE email = $1 AND kind = $2",
            &[&email, &kind],
        )
        .await?;
    Ok(())
}

/// Delete all tokens that have expired.
pub async fn delete_expired_tokens(db: &AnkhDb) -> Result<u64> {
    let deleted = db
        .client
        .execute(
            "DELETE FROM tokens WHERE expires_at < CURRENT_TIMESTAMP",
            &[],
        )
        .await?;
    Ok(deleted)
}
