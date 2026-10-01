//! What the row readers share: an identifier parsed where it is read.
//!
//! The statements select identifiers as text (`id::text`), because
//! `afd_core::id::Uuid7` carries no `sqlx` decoding, so each reader parses
//! them at the row (M-STRONG-TYPES) and nothing past it holds a `String` that
//! might not be one. A stored value that does not parse is a row this daemon
//! cannot read, reported as that, never as the caller's fault.

use afd_core::id::Uuid7;

use crate::sql::COLUMN_ID;
use crate::{Result, error};

/// The table every tenant identifier is reported against.
const TABLE_TENANTS: &str = "core.tenants";

/// The identifier `stored` spells, or a report that `table`'s `column` holds
/// something else.
pub(crate) fn uuid(table: &'static str, column: &'static str, stored: &str) -> Result<Uuid7> {
    Uuid7::parse(stored).map_err(error::row_malformed(table, column))
}

/// A stored tenant identifier.
///
/// Every tenant identifier a reader meets is a `core.tenants.id`, whichever
/// table carried it there, so that is the column a malformed one names.
pub(crate) fn tenant(stored: &str) -> Result<Uuid7> {
    uuid(TABLE_TENANTS, COLUMN_ID, stored)
}

#[cfg(test)]
mod tests {
    use afd_core::error_code;

    use super::{tenant, uuid};

    const STORED: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";

    #[test]
    fn a_stored_identifier_parses_to_itself() {
        let parsed = uuid("core.invites", "id", STORED).map(|id| id.as_str().to_owned());
        assert_eq!(parsed.ok().as_deref(), Some(STORED));
        assert_eq!(
            tenant(STORED)
                .map(|id| id.as_str().to_owned())
                .ok()
                .as_deref(),
            Some(STORED)
        );
    }

    /// A value that is not `UUIDv7` is the datastore holding something this
    /// build cannot read: an internal fault naming the table and column, so
    /// the log says which row to look at.
    #[test]
    fn a_value_that_is_not_an_identifier_names_its_table_and_column() {
        let refused = tenant("not-an-id").err();
        let code = refused.as_ref().map(crate::Error::code);
        assert_eq!(code, Some(error_code::INTERNAL_DB_QUERY));
        let said = refused.map(|error| error.to_string()).unwrap_or_default();
        assert!(said.contains("core.tenants row"), "{said}");
        assert!(said.ends_with(": id"), "{said}");
    }
}
