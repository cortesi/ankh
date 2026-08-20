//! Shared admin pagination helpers.

use ankh_constants::{ADMIN_LIST_DEFAULT_LIMIT, ADMIN_LIST_MAX_LIMIT};

/// Default admin page size.
#[must_use]
pub const fn default_limit() -> i64 {
    ADMIN_LIST_DEFAULT_LIMIT
}

/// Clamp a client-supplied limit into the supported range.
#[must_use]
pub const fn clamp_limit(limit: i64) -> i64 {
    if limit < 1 {
        1
    } else if limit > ADMIN_LIST_MAX_LIMIT {
        ADMIN_LIST_MAX_LIMIT
    } else {
        limit
    }
}
