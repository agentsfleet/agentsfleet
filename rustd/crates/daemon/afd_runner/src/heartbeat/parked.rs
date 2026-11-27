//! The sandboxes a runner says it keeps parked, proved and reconciled on each beat.
//!
//! A runner lists every fleet whose sandbox it keeps parked, frozen. Any park its
//! threads still record that the list leaves out is cleared, so that fleet is
//! assignable everywhere again, and the reply names the listed fleets the
//! runner should let go of. A closing runner's list is final, so even a park
//! reported inside the last beat interval is cleared at once. A list that is unreadable, out of bounds or empty
//! parks nothing: a standing park keeps the fleet from every other runner,
//! while a park cleared by mistake costs only a cold start.

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::runner::ParkedFleets;
use garde::{Unvalidated, Valid};
use sqlx::PgConnection;

use super::best_effort;
use crate::sql;

/// The event a park reconcile that did not land is logged under.
const EVENT_PARKS_RECONCILE: &str = "runner_parks_reconcile_failed";

/// A runner's parked list proved inside the bounds its wire type declares, or
/// `None`: a list past them is not proved, and parks nothing.
#[must_use]
pub fn prove(parked: ParkedFleets<'_>) -> Option<Valid<ParkedFleets<'_>>> {
    Unvalidated::new(parked).validate().ok()
}

/// The fleets a proved list names. An entry inside the bounds that still is
/// not an identifier is dropped alone.
#[must_use]
pub fn fleets(parked: &Valid<ParkedFleets<'_>>) -> Vec<Uuid7> {
    parked
        .0
        .iter()
        .filter_map(|fleet| Uuid7::parse(fleet).ok())
        .collect()
}

/// Clears every park `runner`'s threads record that `parked` leaves out, and
/// answers which listed fleets the runner should release: any that is no
/// longer active, or that another runner has run since. No proved list
/// parks nothing. A `closing` runner's list is final: a park written inside
/// the last beat interval is cleared too, rather than left to a next beat.
pub(super) async fn reconcile(
    connection: &mut PgConnection,
    runner: &Uuid7,
    parked: Option<&Valid<ParkedFleets<'_>>>,
    closing: bool,
    heartbeat_interval: std::time::Duration,
) -> Vec<String> {
    let parked = parked.map_or_default(fleets);
    let listed: Vec<&Uuid7> = parked.iter().collect();
    let clear = sqlx::query(sql::parked::CLEAR_DROPPED_PARKS)
        .bind(runner)
        .bind(&listed)
        .bind(heartbeat_interval)
        .bind(closing);
    best_effort(clear, connection, EVENT_PARKS_RECONCILE, runner).await;
    if listed.is_empty() {
        return Vec::new();
    }
    // A row that will not decode fails the answer like a failed read: the
    // runner keeps its parks one beat longer, and the warning says why.
    let released = sqlx::query_scalar::<_, Uuid7>(sql::parked::SELECT_PARKS_TO_RELEASE)
        .bind(runner)
        .bind(&listed)
        .bind(sql::FLEET_STATUS_ACTIVE)
        .fetch_all(&mut *connection)
        .await;
    match released {
        Ok(fleets) => fleets.into_iter().map(String::from).collect(),
        Err(error) => {
            let code = error_code::INTERNAL_DB_QUERY.as_str();
            let id = runner.as_str();
            let reason = error.to_string();
            let event = EVENT_PARKS_RECONCILE;
            tracing::warn!(error_code = code, runner_id = id, reason, event);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use af_core::id::TEXT_LEN;
    use af_core::limits::MAX_WORKERS;
    use af_wire::runner::{FLEET_ID_TEXT_BYTES, PARKED_MAX, ParkedFleets};

    use super::{fleets, prove};

    /// Two fleets in the canonical version-7 form a runner sends.
    const FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a8057";
    const OTHER_FLEET: &str = "01890a5d-ac96-774b-8cce-b302099a8058";

    /// What a list of `entries` reads as once proved, or `None` when it is not.
    fn read(entries: &[&str]) -> Option<Vec<String>> {
        let parked = ParkedFleets(entries.iter().map(|entry| Cow::Borrowed(*entry)).collect());
        prove(parked).map(|proved| {
            fleets(&proved)
                .iter()
                .map(|fleet| fleet.as_str().to_owned())
                .collect()
        })
    }

    #[test]
    fn test_a_list_past_its_bounds_is_not_proved() {
        assert_eq!(
            read(&vec![FLEET; PARKED_MAX]).map(|read| read.len()),
            Some(PARKED_MAX)
        );
        assert_eq!(read(&vec![FLEET; PARKED_MAX + 1]), None, "past the count");
        assert_eq!(read(&[FLEET, "not-a-fleet"]), None, "an entry too short");
        assert_eq!(read(&[]), Some(Vec::new()), "an empty list parks nothing");
    }

    #[test]
    fn test_an_entry_that_is_no_identifier_is_dropped_alone() {
        let not_an_id = "x".repeat(FLEET_ID_TEXT_BYTES);

        let read = read(&[OTHER_FLEET, &not_an_id, FLEET]);

        assert_eq!(read, Some(vec![OTHER_FLEET.to_owned(), FLEET.to_owned()]));
    }

    /// The wire states its two bounds itself, since it carries no `af_core`;
    /// this is where they are held to the numbers they copy.
    #[test]
    fn test_the_wires_park_bounds_are_the_cores() {
        assert_eq!(
            PARKED_MAX,
            usize::try_from(MAX_WORKERS).unwrap_or(usize::MAX)
        );
        assert_eq!(FLEET_ID_TEXT_BYTES, TEXT_LEN);
    }
}
