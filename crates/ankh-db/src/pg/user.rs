//! User operations.

use ankh_names::normalize_name;

use crate::{
    AnkhDb, Error, NamespaceId, NamespaceKind, ParsedCursor, Result, UserDetail, UserId,
    UserSummary, make_cursor, waitlist_status_from_db, waitlist_status_to_db,
};

/// Adds a new user with a freshly hashed password.
///
/// Creates a namespace for the user atomically. The username must be valid
/// according to namespace naming rules.
pub async fn add_user(db: &mut AnkhDb, username: &str, email: &str, password: &str) -> Result<()> {
    // Validate namespace name
    if let Err(msg) = db.validate_namespace_name(username) {
        return Err(Error::InvalidNamespaceName(msg));
    }

    let normalized = normalize_name(username);

    let password_hash = db.hash_password(password)?;
    let kind = NamespaceKind::User.as_str();

    let tx = db.client.transaction().await?;

    // Create namespace first.
    let namespace_row = tx
        .query_opt(
            "INSERT INTO namespaces (name, kind) VALUES ($1, $2)
         ON CONFLICT (name) DO NOTHING
         RETURNING id",
            &[&normalized, &kind],
        )
        .await?;

    let namespace_id: uuid::Uuid = match namespace_row {
        Some(row) => row.get(0),
        None => return Err(Error::NamespaceExists(normalized)),
    };

    // Create user referencing the namespace.
    let inserted = tx
        .execute(
            "INSERT INTO users (email, namespace_id, password_hash) VALUES ($1, $2, $3)
         ON CONFLICT (email) DO NOTHING",
            &[&email, &namespace_id, &password_hash],
        )
        .await?;

    if inserted == 0 {
        return Err(Error::UserExists(email.to_owned()));
    }

    tx.commit().await?;

    Ok(())
}

/// Gets a user by their username (namespace name).
pub async fn get_user_by_name(db: &AnkhDb, username: &str) -> Result<UserDetail> {
    let normalized = normalize_name(username);

    let row = db
        .client
        .query_opt(
            "SELECT u.id, u.namespace_id, n.name, u.email, u.created_at, u.email_verified_at,
                (SELECT MAX(s.touched_at) FROM sessions s WHERE s.email = u.email)
         FROM users u
         JOIN namespaces n ON n.id = u.namespace_id
         WHERE n.name = $1",
            &[&normalized],
        )
        .await?;

    let row = row.ok_or_else(|| Error::UserMissing(username.to_owned()))?;

    Ok(UserDetail {
        id: UserId(row.get(0)),
        namespace_id: NamespaceId(row.get(1)),
        username: row.get(2),
        email: row.get(3),
        created_at: row.get(4),
        verified_at: row.get(5),
        last_session_at: row.get(6),
    })
}

/// Gets a user by their email address.
pub async fn get_user_by_email(db: &AnkhDb, email: &str) -> Result<UserDetail> {
    let row = db
        .client
        .query_opt(
            "SELECT u.id, u.namespace_id, n.name, u.email, u.created_at, u.email_verified_at,
                (SELECT MAX(s.touched_at) FROM sessions s WHERE s.email = u.email)
         FROM users u
         JOIN namespaces n ON n.id = u.namespace_id
         WHERE u.email = $1",
            &[&email],
        )
        .await?;

    let row = row.ok_or_else(|| Error::UserMissing(email.to_owned()))?;

    Ok(UserDetail {
        id: UserId(row.get(0)),
        namespace_id: NamespaceId(row.get(1)),
        username: row.get(2),
        email: row.get(3),
        created_at: row.get(4),
        verified_at: row.get(5),
        last_session_at: row.get(6),
    })
}

/// Removes a user identified by email, also deleting their personal namespace.
pub async fn delete_user(db: &AnkhDb, email: &str) -> Result<()> {
    // First get the namespace_id so we can clean it up after
    let row = db
        .client
        .query_opt("SELECT namespace_id FROM users WHERE email = $1", &[&email])
        .await?;

    let namespace_id: uuid::Uuid = match row {
        Some(r) => r.get(0),
        None => return Err(Error::UserMissing(email.to_owned())),
    };

    // Delete the user (cascades to sessions, tokens)
    db.client
        .execute("DELETE FROM users WHERE email = $1", &[&email])
        .await?;

    // Delete the orphaned namespace
    db.client
        .execute("DELETE FROM namespaces WHERE id = $1", &[&namespace_id])
        .await?;

    Ok(())
}

/// Updates the stored password hash for an existing user.
pub async fn set_password(db: &AnkhDb, email: &str, password: &str) -> Result<()> {
    let password_hash = db.hash_password(password)?;
    let updated = db
        .client
        .execute(
            "UPDATE users SET password_hash = $1 WHERE email = $2",
            &[&password_hash, &email],
        )
        .await?;

    if updated == 0 {
        return Err(Error::UserMissing(email.to_owned()));
    }

    Ok(())
}

/// Return true if the email address has been verified.
pub async fn is_email_verified(db: &AnkhDb, email: &str) -> Result<bool> {
    let row = db
        .client
        .query_opt(
            "SELECT email_verified_at IS NOT NULL FROM users WHERE email = $1",
            &[&email],
        )
        .await?;
    let row = row.ok_or_else(|| Error::UserMissing(email.to_owned()))?;
    Ok(row.get(0))
}

/// Mark the email address as verified.
pub async fn mark_email_verified(db: &AnkhDb, email: &str) -> Result<()> {
    let updated = db
        .client
        .execute(
            "UPDATE users
         SET email_verified_at = CURRENT_TIMESTAMP
         WHERE email = $1
           AND email_verified_at IS NULL",
            &[&email],
        )
        .await?;

    if updated > 0 {
        return Ok(());
    }

    let exists = db
        .client
        .query_opt("SELECT 1 FROM users WHERE email = $1", &[&email])
        .await?
        .is_some();

    if exists {
        Ok(())
    } else {
        Err(Error::UserMissing(email.to_owned()))
    }
}

/// Return true if the user is waitlisted.
pub async fn is_user_waitlisted(db: &AnkhDb, email: &str) -> Result<bool> {
    let row = db
        .client
        .query_opt(
            "SELECT waitlist_status FROM users WHERE email = $1",
            &[&email],
        )
        .await?;
    let row = row.ok_or_else(|| Error::UserMissing(email.to_owned()))?;
    let status: String = row.get(0);
    waitlist_status_from_db(status.as_str())
}

/// Set whether the user is waitlisted.
pub async fn set_user_waitlisted(db: &AnkhDb, email: &str, waitlisted: bool) -> Result<()> {
    let status = waitlist_status_to_db(waitlisted);
    let updated = db
        .client
        .execute(
            "UPDATE users SET waitlist_status = $1 WHERE email = $2",
            &[&status, &email],
        )
        .await?;

    if updated == 0 {
        return Err(Error::UserMissing(email.to_owned()));
    }

    Ok(())
}

/// Gets a user by ID.
pub async fn get_user_by_id(db: &AnkhDb, id: UserId) -> Result<UserDetail> {
    let row = db
        .client
        .query_opt(
            "SELECT u.id, u.namespace_id, n.name, u.email, u.created_at, u.email_verified_at,
                (SELECT MAX(s.touched_at) FROM sessions s WHERE s.email = u.email)
         FROM users u
         JOIN namespaces n ON n.id = u.namespace_id
         WHERE u.id = $1",
            &[&id.0],
        )
        .await?;

    let row = row.ok_or_else(|| Error::UserMissing(id.to_string()))?;

    Ok(UserDetail {
        id: UserId(row.get(0)),
        namespace_id: NamespaceId(row.get(1)),
        username: row.get(2),
        email: row.get(3),
        created_at: row.get(4),
        verified_at: row.get(5),
        last_session_at: row.get(6),
    })
}

/// Lists users with cursor-based pagination.
pub async fn list_users(
    db: &AnkhDb,
    limit: i64,
    cursor: Option<&str>,
    email_filter: Option<&str>,
) -> Result<(Vec<UserSummary>, Option<String>)> {
    let parsed = cursor
        .map(|c| {
            ParsedCursor::parse(c).ok_or_else(|| Error::UserMissing(format!("invalid cursor: {c}")))
        })
        .transpose()?;
    let filter_pattern = email_filter.map(|f| format!("%{f}%"));

    // Fetch one extra row to learn whether a further page exists without
    // emitting a cursor that points at an empty page.
    let fetch_limit = limit + 1;

    let rows = match (&parsed, &filter_pattern) {
        (Some(c), Some(fp)) => db
            .client
            .query(
                "SELECT u.id, u.namespace_id, n.name, u.email, u.created_at, u.email_verified_at
             FROM users u
             JOIN namespaces n ON n.id = u.namespace_id
             WHERE (u.created_at, u.id) < ($1, $2) AND u.email ILIKE $3
             ORDER BY u.created_at DESC, u.id DESC LIMIT $4",
                &[&c.time, &c.id, fp, &fetch_limit],
            )
            .await?,
        (Some(c), None) => db
            .client
            .query(
                "SELECT u.id, u.namespace_id, n.name, u.email, u.created_at, u.email_verified_at
             FROM users u
             JOIN namespaces n ON n.id = u.namespace_id
             WHERE (u.created_at, u.id) < ($1, $2)
             ORDER BY u.created_at DESC, u.id DESC LIMIT $3",
                &[&c.time, &c.id, &fetch_limit],
            )
            .await?,
        (None, Some(fp)) => db
            .client
            .query(
                "SELECT u.id, u.namespace_id, n.name, u.email, u.created_at, u.email_verified_at
             FROM users u
             JOIN namespaces n ON n.id = u.namespace_id
             WHERE u.email ILIKE $1 ORDER BY u.created_at DESC, u.id DESC LIMIT $2",
                &[fp, &fetch_limit],
            )
            .await?,
        (None, None) => db
            .client
            .query(
                "SELECT u.id, u.namespace_id, n.name, u.email, u.created_at, u.email_verified_at
             FROM users u
             JOIN namespaces n ON n.id = u.namespace_id
             ORDER BY u.created_at DESC, u.id DESC LIMIT $1",
                &[&fetch_limit],
            )
            .await?,
    };

    let has_more = rows.len() as i64 > limit;
    let users: Vec<UserSummary> = rows
        .iter()
        .take(limit as usize)
        .map(|row| {
            let id: uuid::Uuid = row.get(0);
            UserSummary {
                id: UserId(id),
                namespace_id: NamespaceId(row.get(1)),
                username: row.get(2),
                email: row.get(3),
                created_at: row.get(4),
                verified_at: row.get(5),
            }
        })
        .collect();

    let next_cursor = has_more
        .then(|| users.last().map(|u| make_cursor(&u.created_at, &u.id.0)))
        .flatten();

    Ok((users, next_cursor))
}

/// Deletes a user by ID, cascading to sessions and deleting the personal namespace.
pub async fn delete_user_by_id(db: &AnkhDb, id: UserId) -> Result<()> {
    let row = db
        .client
        .query_opt(
            "SELECT email, namespace_id FROM users WHERE id = $1",
            &[&id.0],
        )
        .await?;

    let row = row.ok_or_else(|| Error::UserMissing(id.to_string()))?;
    let email: String = row.get(0);
    let namespace_id: uuid::Uuid = row.get(1);

    // Delete the user (cascades to sessions, tokens)
    db.client
        .execute("DELETE FROM users WHERE email = $1", &[&email])
        .await?;

    // Delete the orphaned namespace
    db.client
        .execute("DELETE FROM namespaces WHERE id = $1", &[&namespace_id])
        .await?;

    Ok(())
}
