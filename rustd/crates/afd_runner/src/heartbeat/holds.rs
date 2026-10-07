//! The sandboxes a runner says it holds, reconciled on each beat.
//!
//! A runner lists every fleet whose sandbox it holds frozen. Any hold its
//! slots still record that the list leaves out is cleared, so that fleet is
//! claimable everywhere again, and the reply names the listed fleets the
//! runner should let go of. Lenient like the rest of the beat: a list past
//! its bound reads as nothing reported, and reconciles nothing.

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::runner::HeldFleets;
use garde::Validate as _;
use sqlx::{PgConnection, Row as _};

use super::best_effort;
use crate::sql;

/// The event a hold reconcile that did not land is logged under.
const EVENT_HOLDS_RECONCILE: &str = "runner_holds_reconcile_failed";

/// Clears every hold `runner`'s slots record that `holds` leaves out, and
/// answers which listed fleets the runner should release: any that is no
/// longer active, or that another runner has leased since.
pub(super) async fn reconcile(
    connection: &mut PgConnection,
    runner: &Uuid7,
    holds: &HeldFleets<'_>,
    now: UnixMillis,
) -> Vec<String> {
    if holds.validate().is_err() {
        return Vec::new();
    }
    let listed: Vec<&str> = holds
        .0
        .iter()
        .map(AsRef::as_ref)
        .filter(|fleet| Uuid7::parse(fleet).is_ok())
        .collect();
    let clear = sqlx::query(sql::holds::CLEAR_DROPPED_HOLDS)
        .bind(runner.as_str())
        .bind(&listed)
        .bind(now.as_millis());
    best_effort(clear, connection, EVENT_HOLDS_RECONCILE, runner).await;
    if listed.is_empty() {
        return Vec::new();
    }
    // A row that will not decode fails the answer like a failed read: the
    // runner keeps its holds one beat longer, and the warning says why.
    let released = sqlx::query(sql::holds::SELECT_HOLDS_TO_RELEASE)
        .bind(runner.as_str())
        .bind(&listed)
        .bind(sql::FLEET_STATUS_ACTIVE)
        .fetch_all(&mut *connection)
        .await
        .and_then(|rows| {
            rows.iter()
                .map(|row| row.try_get::<String, _>(0))
                .collect::<Result<Vec<_>, _>>()
        });
    match released {
        Ok(fleets) => fleets,
        Err(error) => {
            let code = error_code::INTERNAL_DB_QUERY.as_str();
            let id = runner.as_str();
            let reason = error.to_string();
            let event = EVENT_HOLDS_RECONCILE;
            tracing::warn!(error_code = code, runner_id = id, reason, event);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use afd_core::id::TEXT_LEN;
    use afd_core::limits::MAX_WORKERS;
    use afd_wire::runner::{FLEET_ID_TEXT_BYTES, HOLDS_MAX};

    /// The wire states its two bounds itself, since it carries no `afd_core`;
    /// this is where they are held to the numbers they copy.
    #[test]
    fn test_the_wires_hold_bounds_are_the_cores() {
        assert_eq!(
            HOLDS_MAX,
            usize::try_from(MAX_WORKERS).unwrap_or(usize::MAX)
        );
        assert_eq!(FLEET_ID_TEXT_BYTES, TEXT_LEN);
    }
}
