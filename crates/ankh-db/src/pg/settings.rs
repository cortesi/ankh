//! Settings and schema version operations.

use crate::{AnkhDb, AppSettings, Result};

/// Returns the global application settings.
pub async fn get_app_settings(db: &AnkhDb) -> Result<AppSettings> {
    db.ensure_settings_row().await?;
    let row = db
        .client
        .query_one(
            "SELECT waitlist_enabled FROM ankh_settings WHERE id = 1",
            &[],
        )
        .await?;
    Ok(AppSettings {
        waitlist_enabled: row.get(0),
    })
}

/// Updates the waitlist-enabled setting and returns the updated settings.
pub async fn set_waitlist_enabled(db: &AnkhDb, enabled: bool) -> Result<AppSettings> {
    db.ensure_settings_row().await?;
    let row = db
        .client
        .query_one(
            "UPDATE ankh_settings
         SET waitlist_enabled = $1, updated_at = CURRENT_TIMESTAMP
         WHERE id = 1
         RETURNING waitlist_enabled",
            &[&enabled],
        )
        .await?;
    Ok(AppSettings {
        waitlist_enabled: row.get(0),
    })
}
