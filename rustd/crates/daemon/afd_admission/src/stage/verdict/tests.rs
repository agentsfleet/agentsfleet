//! The refusal order `decide` names, one fact set at a time.

use afd_fleet_lifecycle::FleetStatus;

use super::{Answer, Bound, decide};
use crate::Error;

type Outcome = std::result::Result<(), Box<dyn std::error::Error>>;

/// The refusal `decide` raises for `answer`, or a failed test when it admits.
fn refusal(
    answer: &Answer,
    bound: Bound,
) -> std::result::Result<Error, Box<dyn std::error::Error>> {
    decide(answer, bound)
        .err()
        .ok_or_else(|| "the answer was admitted, not refused".into())
}

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
        run: super::RunRow::default(),
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
fn verdict_names_the_first_refusal_in_order() -> Outcome {
    let mut absent = stored(open(), false);
    absent.fleet_found = false;
    assert!(refusal(&absent, RUNNABLE_OPEN)?.is_fleet_absent());

    let repeat = decide(&stored(stopped(open()), false), RUNNABLE_CLOSED)?;
    assert!(
        !repeat.inserted,
        "a stored row on a stopped fleet answers the repeat"
    );

    let mut foreign_marker = stopped(open());
    foreign_marker.setup_current = false;
    assert_eq!(
        refusal(&foreign_marker, RUNNABLE_OPEN)?.not_runnable(),
        Some(FleetStatus::Stopped)
    );

    let mut policy = open();
    policy.setup_current = false;
    assert_eq!(
        refusal(&policy, RUNNABLE_CLOSED)?.code(),
        af_core::error_code::AUTHORING_POLICY
    );

    assert!(refusal(&open(), RUNNABLE_CLOSED)?.is_over_capacity());

    assert!(decide(&stored(open(), true), RUNNABLE_OPEN)?.inserted);
    Ok(())
}

#[test]
fn an_unbound_status_admits_an_installing_fleet() -> Outcome {
    let mut installing = stored(open(), true);
    installing.status = Some(FleetStatus::Installing.as_str().to_owned());
    let staged = Bound {
        runnable: None,
        ..RUNNABLE_OPEN
    };
    assert!(decide(&installing, staged)?.inserted);
    Ok(())
}

#[test]
fn an_unknown_status_or_a_missing_row_is_unreadable() -> Outcome {
    let mut unknown = open();
    unknown.status = Some("archived".to_owned());
    assert_eq!(
        refusal(&unknown, RUNNABLE_OPEN)?.code(),
        af_core::error_code::INTERNAL_DB_QUERY
    );

    assert_eq!(
        refusal(&open(), RUNNABLE_OPEN)?.code(),
        af_core::error_code::INTERNAL_DB_QUERY,
        "open gates that wrote no row are a broken answer, never an accept"
    );
    Ok(())
}
