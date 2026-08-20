//! Embedded canonical Ankh identity schema.

/// Current Ankh schema version inserted by [`crate::AnkhDb::initialize`].
pub const ANKH_SCHEMA_VERSION: i32 = 1;

/// Canonical Ankh identity schema SQL.
///
/// Returned by function rather than exposed as a constant so the embedded SQL
/// does not pollute the rendered documentation.
#[must_use]
pub fn schema_sql() -> &'static str {
    include_str!("../sql/schema.sql")
}
