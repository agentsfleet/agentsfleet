#![expect(
    clippy::panic,
    reason = "fixed test fixtures fail loudly when their contract changes"
)]

use afd_api_wire::schedule::RunState;
use afd_core::id::Uuid7;
use jiff::Timestamp;

use super::{describe, upcoming};
use crate::model::{DesiredStatus, Schedule, Source, SyncStatus};

/// 2026-03-15T09:04:30Z.
const NOW: Timestamp = Timestamp::constant(1_773_565_470, 0);

/// 2026-03-15T09:05:00Z, the next minute after [`NOW`].
const NEXT_MINUTE: Timestamp = Timestamp::constant(1_773_565_500, 0);

/// 2026-03-15T09:04:00Z, a minute already behind [`NOW`].
const PASSED: Timestamp = Timestamp::constant(1_773_565_440, 0);

/// A schedule at `desired` and `sync`, firing on `cron` in UTC.
fn schedule(desired: DesiredStatus, sync: SyncStatus, cron: &str) -> Schedule {
    let id = Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010")
        .unwrap_or_else(|error| panic!("fixture id is canonical: {error}"));
    Schedule {
        schedule_id: id.clone(),
        fleet_id: id,
        source: Source::Api,
        provider_schedule_id: "fixture".to_owned(),
        cron: cron.to_owned(),
        timezone: "UTC".to_owned(),
        message: "Run the fixture.".to_owned(),
        once: false,
        desired_status: desired,
        sync_status: sync,
        resource_version: 1,
        sync_token: None,
        sync_lease_expires_at: None,
        last_error: None,
        created_at: NOW,
        updated_at: NOW,
        fire_at: None,
        recipe: None,
    }
}

/// A one-off set for `fire_at`, registered and active.
fn one_off(fire_at: Option<Timestamp>) -> Schedule {
    Schedule {
        once: true,
        fire_at,
        ..schedule(DesiredStatus::Active, SyncStatus::Synced, "5 9 15 3 *")
    }
}

#[test]
fn every_status_pair_reads_one_run_state() {
    use DesiredStatus::{Active, Deleting, Paused};
    use SyncStatus::{Failed, Synced, Syncing};
    let cases = [
        (Active, Synced, RunState::Scheduled, Some(NEXT_MINUTE)),
        (Active, Syncing, RunState::ActivationPending, None),
        (Active, Failed, RunState::NextRunUnavailable, None),
        (Paused, Synced, RunState::Paused, None),
        (Paused, Syncing, RunState::Paused, None),
        (Paused, Failed, RunState::Paused, None),
        (Deleting, Synced, RunState::NextRunUnavailable, None),
        (Deleting, Syncing, RunState::NextRunUnavailable, None),
        (Deleting, Failed, RunState::NextRunUnavailable, None),
    ];
    for (desired, sync, state, next) in cases {
        assert_eq!(
            upcoming(&schedule(desired, sync, "* * * * *"), NOW),
            (state, next),
            "{desired:?} and {sync:?}: only an active, registered schedule names a next run"
        );
    }
}

#[test]
fn a_one_off_runs_at_its_stored_instant_and_not_after_it() {
    for (fire_at, expected) in [
        (Some(NEXT_MINUTE), (RunState::Scheduled, Some(NEXT_MINUTE))),
        (Some(PASSED), (RunState::NextRunUnavailable, None)),
        (Some(NOW), (RunState::NextRunUnavailable, None)),
        (None, (RunState::NextRunUnavailable, None)),
    ] {
        assert_eq!(
            upcoming(&one_off(fire_at), NOW),
            expected,
            "a one-off at {fire_at:?} never borrows its expression's match a year out"
        );
    }
}

#[test]
fn a_recurring_expression_with_no_future_match_has_no_next_run() {
    // February 30th never exists: registered and active, but it will not run.
    let impossible = schedule(DesiredStatus::Active, SyncStatus::Synced, "0 0 30 2 *");
    assert_eq!(
        upcoming(&impossible, NOW),
        (RunState::NextRunUnavailable, None)
    );
}

#[test]
fn a_description_names_the_expression_in_words_and_its_zone() {
    assert_eq!(
        describe("*/15 * * * *", "Asia/Kolkata"),
        "Every 15 minutes (Asia/Kolkata)"
    );
    assert_eq!(
        describe("not a cron", "UTC"),
        "not a cron (UTC)",
        "an expression the parser refuses is shown as written, never hidden"
    );
}

#[test]
fn a_time_in_the_midnight_hour_keeps_its_minute() {
    for (cron, words) in [
        ("30 0 * * *", "At 12:30 AM"),
        ("5 0 * * *", "At 12:05 AM"),
        ("0 0 * * *", "At 12:00 AM"),
        ("30 9 * * *", "At 9:30 AM"),
    ] {
        let described = describe(cron, "UTC");
        assert!(described.starts_with(words), "{cron}: {described}");
    }
}
