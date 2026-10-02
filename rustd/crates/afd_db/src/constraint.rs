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

#[cfg(test)]
mod tests {
    use super::violates_unique;

    /// The true arm needs a `DatabaseError` carrying SQLSTATE 23505 and the
    /// constraint, which sqlx only produces from a real driver; the callers'
    /// live suites hold it. This holds the arm that decides whether a broken
    /// statement is mistaken for a conflict, and a caller that retries on a
    /// conflict would loop on a fault that never clears.
    #[test]
    fn a_failure_with_no_database_behind_it_violates_nothing() {
        assert!(!violates_unique(&sqlx::Error::PoolClosed, "uq_anything"));
        assert!(!violates_unique(&sqlx::Error::RowNotFound, "uq_anything"));
    }
}
