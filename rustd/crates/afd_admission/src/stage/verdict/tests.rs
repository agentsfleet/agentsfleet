//! The refusal order `decide` names, one fact set at a time (`M242_001` Dimension 2.7).
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet_lifecycle::FleetStatus;

use super::{Answer, Bound, decide};

/// An answer for a found, active, current fleet with no stored row.
fn open() -> Answer {
    Answer {
        fleet_found: true,
        status: Some(FleetStatus::Active.as_str().to_owned()),
        setup_current: true,
        inserted: false,
        id: None,
        digest: None,
        fleet: None,
        created_at: None,
    }
}

/// The same answer carrying a stored row.
fn stored(mut answer: Answer, inserted: bool) -> Answer {
    answer.inserted = inserted;
    answer.id = Some("0190d5c2-7e1a-7000-8000-000000000001".to_owned());
    answer.digest = Some("digest".to_owned());
    answer.fleet = Some("0190d5c2-7e1a-7000-8000-000000000002".to_owned());
    answer.created_at = Some(jiff::Timestamp::UNIX_EPOCH.into());
    answer
}

const RUNNABLE_OPEN: Bound = Bound {
    runnable: Some(FleetStatus::Active),
    ceiling_open: true,
    limit: 10,
};

const RUNNABLE_CLOSED: Bound = Bound {
    ceiling_open: false,
    ..RUNNABLE_OPEN
};

fn stopped(mut answer: Answer) -> Answer {
    answer.status = Some(FleetStatus::Stopped.as_str().to_owned());
    answer
}

#[test]
fn verdict_names_the_first_refusal_in_order() {
    let mut absent = stored(open(), false);
    absent.fleet_found = false;
    assert!(
        decide(&absent, RUNNABLE_OPEN)
            .unwrap_err()
            .is_fleet_absent()
    );

    let repeat = decide(&stored(stopped(open()), false), RUNNABLE_CLOSED).unwrap();
    assert!(
        !repeat.inserted,
        "a stored row on a stopped fleet answers the repeat"
    );

    let mut foreign_marker = stopped(open());
    foreign_marker.setup_current = false;
    assert_eq!(
        decide(&foreign_marker, RUNNABLE_OPEN)
            .unwrap_err()
            .not_runnable(),
        Some(FleetStatus::Stopped)
    );

    let mut policy = open();
    policy.setup_current = false;
    let refused = decide(&policy, RUNNABLE_CLOSED).unwrap_err();
    assert_eq!(refused.code(), afd_core::error_code::AUTHORING_POLICY);

    let ceiling = decide(&open(), RUNNABLE_CLOSED).unwrap_err();
    assert!(ceiling.is_over_capacity());

    assert!(
        decide(&stored(open(), true), RUNNABLE_OPEN)
            .unwrap()
            .inserted
    );
}

#[test]
fn an_unbound_status_admits_an_installing_fleet() {
    let mut installing = stored(open(), true);
    installing.status = Some(FleetStatus::Installing.as_str().to_owned());
    let staged = Bound {
        runnable: None,
        ..RUNNABLE_OPEN
    };
    assert!(decide(&installing, staged).unwrap().inserted);
}

#[test]
fn an_unknown_status_or_a_missing_row_is_unreadable() {
    let mut unknown = open();
    unknown.status = Some("archived".to_owned());
    let refused = decide(&unknown, RUNNABLE_OPEN).unwrap_err();
    assert_eq!(refused.code(), afd_core::error_code::INTERNAL_DB_QUERY);

    let refused = decide(&open(), RUNNABLE_OPEN).unwrap_err();
    assert_eq!(
        refused.code(),
        afd_core::error_code::INTERNAL_DB_QUERY,
        "open gates that wrote no row are a broken answer, never an accept"
    );
}
