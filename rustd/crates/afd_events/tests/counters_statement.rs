//! The counter columns ride every closing, and the standalone read never joins.
//!
//! Source-level pins, no datastore: the invariant is the statement text, and
//! a build that drops the counters from one closing statement compiles just
//! as well as one that keeps them — the decoder would fail at runtime on the
//! first ended event, which is the wrong place to learn it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_events::sql::{
    SELECT_FLEET_COUNTERS, UPDATE_FLEET_EVENT_FAILURE, UPDATE_FLEET_EVENT_RESULT,
};

const EVENTS_PROCESSED: &str = "AS events_processed";
const BUDGET_USED_NANOS: &str = "AS budget_used_nanos";
const PENDING_APPROVALS: &str = "AS pending_approvals";
const COUNTERS_TABLE: &str = "core.fleet_activity_counters";
const JOIN: &str = "JOIN";

/// Both closing statements answer the counters, after the pending count, in
/// one order — the decoder reads them positionally off `EventRow::COLUMNS`.
#[test]
fn both_closings_carry_the_counters_after_the_pending_count() {
    for (name, statement) in [
        ("UPDATE_FLEET_EVENT_FAILURE", UPDATE_FLEET_EVENT_FAILURE),
        ("UPDATE_FLEET_EVENT_RESULT", UPDATE_FLEET_EVENT_RESULT),
    ] {
        for column in [PENDING_APPROVALS, EVENTS_PROCESSED, BUDGET_USED_NANOS] {
            assert!(statement.contains(column), "{name} lost {column}");
        }
        let at = |column: &str| statement.find(column).expect("asserted present above");
        let (pending, events, budget) = (
            at(PENDING_APPROVALS),
            at(EVENTS_PROCESSED),
            at(BUDGET_USED_NANOS),
        );
        assert!(
            pending < events && events < budget,
            "{name}: the columns must sit in decoder order"
        );
    }
}

/// The standalone read reaches the row by primary key and never joins — an
/// inner join would lose a fleet that has never run, and an outer one plans
/// as a scan of the whole table.
#[test]
fn the_counter_read_is_a_key_lookup_and_never_a_join() {
    assert!(SELECT_FLEET_COUNTERS.contains(COUNTERS_TABLE));
    assert!(SELECT_FLEET_COUNTERS.contains(EVENTS_PROCESSED));
    assert!(SELECT_FLEET_COUNTERS.contains(BUDGET_USED_NANOS));
    assert!(
        !SELECT_FLEET_COUNTERS.contains(JOIN),
        "{SELECT_FLEET_COUNTERS}"
    );
}
