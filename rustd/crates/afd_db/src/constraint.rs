//! Telling one named constraint's violation apart from any other failure.

/// Whether `error` is a unique violation of exactly `constraint`.
///
/// By name, not by SQLSTATE alone: a table's primary key is unique too, and an
/// identifier collision is not a fact about the column a caller guards, so it
/// must surface as the failure it is rather than as that caller's conflict.
#[must_use]
pub fn violates_unique(error: &sqlx::Error, constraint: &str) -> bool {
    error.as_database_error().is_some_and(|failure| {
        failure.is_unique_violation() && failure.constraint() == Some(constraint)
    })
}

/// SQLSTATE `foreign_key_violation`: a referencing row names no referenced row,
/// or a `NO ACTION` key is left dangling at statement end.
const FOREIGN_KEY_VIOLATION: &str = "23503";

/// SQLSTATE `restrict_violation`: a delete or key update on a row an
/// `ON DELETE RESTRICT` key still references. sqlx's
/// `is_foreign_key_violation` reads 23503 only, so this one is matched by code.
const RESTRICT_VIOLATION: &str = "23001";

/// Whether `error` is a foreign-key violation of exactly `constraint`, from
/// either end: an insert naming a missing row, or a delete of a row a
/// `RESTRICT` key still holds.
///
/// By name for the reason [`violates_unique`] is: a table with two foreign keys
/// answers the same SQLSTATE for either, and only the caller knows which one
/// is the fact it reports.
#[must_use]
pub fn violates_foreign_key(error: &sqlx::Error, constraint: &str) -> bool {
    error.as_database_error().is_some_and(|failure| {
        failure
            .code()
            .is_some_and(|code| code == FOREIGN_KEY_VIOLATION || code == RESTRICT_VIOLATION)
            && failure.constraint() == Some(constraint)
    })
}

#[cfg(test)]
mod tests {
    use super::{violates_foreign_key, violates_unique};

    /// The true arm needs a `DatabaseError` carrying SQLSTATE 23505 and the
    /// constraint, which sqlx only produces from a real driver; the callers'
    /// live suites hold it. This holds the arm that decides whether a broken
    /// statement is mistaken for a conflict, and a caller that retries on a
    /// conflict would loop on a fault that never clears.
    #[test]
    fn a_failure_with_no_database_behind_it_violates_nothing() {
        assert!(!violates_unique(&sqlx::Error::PoolClosed, "uq_anything"));
        assert!(!violates_unique(&sqlx::Error::RowNotFound, "uq_anything"));
        assert!(!violates_foreign_key(
            &sqlx::Error::PoolClosed,
            "fk_anything"
        ));
        assert!(!violates_foreign_key(
            &sqlx::Error::RowNotFound,
            "fk_anything"
        ));
    }
}
