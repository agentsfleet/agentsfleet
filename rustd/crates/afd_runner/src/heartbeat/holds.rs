//! The sandboxes a runner says it holds, proved and reconciled on each beat.
//!
//! A runner lists every fleet whose sandbox it holds frozen. Any hold its
//! slots still record that the list leaves out is cleared, so that fleet is
//! claimable everywhere again, and the reply names the listed fleets the
//! runner should let go of. A closing runner's list is final, so even a hold
//! reported inside the last beat interval is cleared at once. A list that is unreadable, out of bounds or empty
//! holds nothing: a standing hold keeps the fleet from every other runner,
//! while a hold cleared by mistake costs only a cold start.

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::timing::HEARTBEAT_INTERVAL_MS;
use afd_wire::runner::HeldFleets;
use garde::{Unvalidated, Valid};
use sqlx::{PgConnection, Row as _};

use super::best_effort;
use crate::sql;

/// The event a hold reconcile that did not land is logged under.
const EVENT_HOLDS_RECONCILE: &str = "runner_holds_reconcile_failed";

/// A runner's holds list proved inside the bounds its wire type declares, or
/// `None`: a list past them is not proved, and holds nothing.
#[must_use]
pub fn prove(holds: HeldFleets<'_>) -> Option<Valid<HeldFleets<'_>>> {
    Unvalidated::new(holds).validate().ok()
}

/// The fleets a proved list names. An entry inside the bounds that still is
/// not an identifier is dropped alone.
#[must_use]
pub fn fleets(holds: &Valid<HeldFleets<'_>>) -> Vec<Uuid7> {
    holds
        .0
        .iter()
        .filter_map(|fleet| Uuid7::parse(fleet).ok())
        .collect()
}

/// Clears every hold `runner`'s slots record that `holds` leaves out, and
/// answers which listed fleets the runner should release: any that is no
/// longer active, or that another runner has leased since. No proved list
/// holds nothing. A `closing` runner's list is final: a hold written inside
/// the last beat interval is cleared too, rather than left to a next beat.
pub(super) async fn reconcile(
    connection: &mut PgConnection,
    runner: &Uuid7,
    holds: Option<&Valid<HeldFleets<'_>>>,
    closing: bool,
    now: UnixMillis,
) -> Vec<String> {
    let held = holds.map(fleets).unwrap_or_default();
    let listed: Vec<&str> = held.iter().map(Uuid7::as_str).collect();
    let clear = sqlx::query(sql::holds::CLEAR_DROPPED_HOLDS)
        .bind(runner.as_str())
        .bind(&listed)
        .bind(now.as_millis())
        .bind(HEARTBEAT_INTERVAL_MS)
        .bind(closing);
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
    use std::borrow::Cow;

    use afd_core::id::TEXT_LEN;
    use afd_core::limits::MAX_WORKERS;
    use afd_wire::runner::{FLEET_ID_TEXT_BYTES, HOLDS_MAX, HeldFleets};

    use super::{fleets, prove};

    /// Two fleets in the canonical version-7 form a runner sends.
    const FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a8057";
    const OTHER_FLEET: &str = "01890a5d-ac96-774b-8cce-b302099a8058";

    /// What a list of `entries` reads as once proved, or `None` when it is not.
    fn read(entries: &[&str]) -> Option<Vec<String>> {
        let holds = HeldFleets(entries.iter().map(|entry| Cow::Borrowed(*entry)).collect());
        prove(holds).map(|proved| {
            fleets(&proved)
                .iter()
                .map(|fleet| fleet.as_str().to_owned())
                .collect()
        })
    }

    #[test]
    fn test_a_list_past_its_bounds_is_not_proved() {
        assert_eq!(
            read(&vec![FLEET; HOLDS_MAX]).map(|read| read.len()),
            Some(HOLDS_MAX)
        );
        assert_eq!(read(&vec![FLEET; HOLDS_MAX + 1]), None, "past the count");
        assert_eq!(read(&[FLEET, "not-a-fleet"]), None, "an entry too short");
        assert_eq!(read(&[]), Some(Vec::new()), "an empty list holds nothing");
    }

    #[test]
    fn test_an_entry_that_is_no_identifier_is_dropped_alone() {
        let not_an_id = "x".repeat(FLEET_ID_TEXT_BYTES);

        let read = read(&[OTHER_FLEET, &not_an_id, FLEET]);

        assert_eq!(read, Some(vec![OTHER_FLEET.to_owned(), FLEET.to_owned()]));
    }

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
