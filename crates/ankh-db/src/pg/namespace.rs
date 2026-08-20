//! Namespace state queries.

use crate::{AnkhDb, Error, NamespaceId, NamespaceStatusUpdate, Result};

/// Set namespace suspension state and bump its edge-visible generation.
pub async fn set_namespace_suspended(
    db: &AnkhDb,
    namespace_id: NamespaceId,
    suspended: bool,
) -> Result<NamespaceStatusUpdate> {
    let status = if suspended { "suspended" } else { "active" };
    let row = db
        .client()
        .query_opt(
            "UPDATE namespaces
             SET status = $2, gen = gen + 1
             WHERE id = $1
             RETURNING name, status, gen",
            &[&namespace_id.0, &status],
        )
        .await?
        .ok_or_else(|| Error::NamespaceMissing(namespace_id.to_string()))?;

    let status: String = row.get(1);
    Ok(NamespaceStatusUpdate {
        name: row.get(0),
        suspended: status == "suspended",
        r#gen: row.get(2),
    })
}
