//! What one contended window writes into its report (`M241_001` §3).

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use core::time::Duration;

use super::{CANDIDATES_PER_POLL, Window};
use crate::instrument::PollCounters;
use crate::lane::outcomes::Outcomes;
use crate::profile::Profile;
use crate::report::{Lane, Provenance, Report};

/// A window whose poll counters and statement count disagree on purpose.
fn window(statements: u64) -> Window {
    Window {
        outcomes: Outcomes::new().expect("an empty outcome set builds"),
        length: Duration::from_secs(1),
        exhausted: false,
        counters: PollCounters {
            polls: 4,
            candidates: 12,
        },
        statements,
        dragonfly_calls: 0,
    }
}

fn recorded(window: &Window) -> Report {
    let mut report = Report::new(Lane::Lease, Profile::Rig, Provenance::for_test());
    window.record(&mut report);
    report
}

#[test]
fn window_measurements_carry_no_counter_roundtrips() {
    let report = recorded(&window(120));

    assert!(
        !report.measurements.contains_key("roundtrips_per_lease"),
        "a poll-kept tally missed most statements; none may be reported"
    );
    assert_eq!(
        report.measurements.get(CANDIDATES_PER_POLL).copied(),
        Some(3.0),
        "fan-out per poll is the counter that stays: 12 candidates over 4 polls"
    );
}

#[test]
fn window_operations_come_from_statements() {
    let report = recorded(&window(120));

    assert_eq!(
        report.datastores.postgres.operations, 120,
        "Postgres operations are what pg_stat_statements counted in the window"
    );
}
