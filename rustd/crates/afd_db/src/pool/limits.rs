//! The time limits every pooled runtime connection carries.
//!
//! Sent as startup options (`-c lock_timeout=…`), so they hold from the first
//! statement and need no round trip of their own. A session `SET` would leak
//! across clients behind the API role's transaction pooler, and a per-role
//! setting on `api_runtime` never fires because that role cannot log in.
//!
//! The migrator sets neither: a migration legitimately waits on locks and runs
//! long statements, and its session advisory lock already serialises it.

use std::time::Duration;

use sqlx::postgres::PgConnectOptions;

use crate::config::DbRole;

/// How long a pooled statement waits on a lock before Postgres gives up with
/// `55P03 lock_not_available`.
///
/// Well inside [`STATEMENT_TIMEOUT`], so a request blocked behind a held row
/// fails retryably long before it spends the whole statement budget.
pub const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

/// The longest a pooled statement runs before Postgres cancels it with
/// `57014 query_canceled`.
///
/// The whole setup budget (`SETUP_BUDGET` in `afd_authoring`): a statement
/// that outlives it has already failed the request it serves.
pub const STATEMENT_TIMEOUT: Duration = Duration::from_secs(15);

/// SQLSTATE `lock_not_available`: a statement outwaited [`LOCK_TIMEOUT`].
const LOCK_NOT_AVAILABLE: &str = "55P03";

/// Whether `error`, or anything in its cause chain, is a statement that gave
/// up waiting on a lock.
///
/// The one retryable refusal the limit produces: the row was busy, nothing
/// was written, and the same request may be sent again. Walks the chain so a
/// crate's own error type answers without unwrapping itself first.
#[must_use]
pub fn gave_up_on_lock(error: &(dyn std::error::Error + 'static)) -> bool {
    std::iter::successors(Some(error), |current| current.source()).any(|current| {
        current
            .downcast_ref::<sqlx::Error>()
            .and_then(sqlx::Error::as_database_error)
            .and_then(sqlx::error::DatabaseError::code)
            .is_some_and(|code| code == LOCK_NOT_AVAILABLE)
    })
}

/// `options` with both limits added, unless `role` is the migrator.
pub(crate) fn bounded(role: DbRole, options: PgConnectOptions) -> PgConnectOptions {
    if role == DbRole::Migrator {
        return options;
    }
    options.options([
        ("lock_timeout", setting(LOCK_TIMEOUT)),
        ("statement_timeout", setting(STATEMENT_TIMEOUT)),
    ])
}

/// A span in the unit-suffixed form Postgres reads for a time setting.
fn setting(span: Duration) -> String {
    format!("{}ms", span.as_millis())
}

#[cfg(test)]
mod tests {
    use super::{LOCK_TIMEOUT, STATEMENT_TIMEOUT, gave_up_on_lock, setting};

    #[test]
    fn a_lock_wait_gives_up_well_inside_the_statement_budget() {
        assert!(LOCK_TIMEOUT * 2 <= STATEMENT_TIMEOUT);
    }

    /// The true arm needs a driver-made `DatabaseError` carrying 55P03; the
    /// billing and authoring live suites hold it.
    #[test]
    fn a_failure_with_no_database_behind_it_is_not_a_lock_wait() {
        assert!(!gave_up_on_lock(&sqlx::Error::PoolTimedOut));
        assert!(!gave_up_on_lock(&std::io::Error::other("unrelated")));
    }

    #[test]
    fn a_span_is_written_in_whole_milliseconds_with_its_unit() {
        assert_eq!(setting(LOCK_TIMEOUT), "2000ms");
        assert_eq!(setting(STATEMENT_TIMEOUT), "15000ms");
    }
}
