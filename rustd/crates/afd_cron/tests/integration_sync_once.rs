//! A one-off reconciled after its moment is retired, not registered a year late.
//!
//! A `once` schedule's expression has no year. A create that cannot reach the
//! scheduler leaves the row `failed`, and a later sync pushes it again; past
//! the minute, the next match of that expression is a year away. The row keeps
//! the instant it was set for (slot 931), and the reconciler removes a one-off
//! whose instant has passed instead of registering it.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

#[path = "support/live_qstash.rs"]
mod live_qstash;

use afd_core::clock::UnixMillis;
use afd_cron::{Change, NewSchedule, Reconciled, Schedule, Source};

use self::live_qstash::{against_live, live};
use self::support::CronLane;

/// 2026-03-15T09:04:30Z: when the one-off was asked for.
const ASKED_AT: i64 = 1_773_565_470_000;

/// 2026-03-15T09:05:00Z: the minute it was set for.
const SET_FOR: i64 = 1_773_565_500_000;

/// One minute, in milliseconds: how late the sync below runs.
const MINUTE_MS: i64 = 60_000;

/// The expression for [`SET_FOR`], in UTC.
const SET_FOR_CRON: &str = "5 9 15 3 *";

/// A one-off created at [`ASKED_AT`], claimed by its creator.
async fn one_off(lane: &CronLane, token: &afd_core::id::Uuid7) -> Schedule {
    let fleet = lane.fleet_id();
    lane.store
        .create(
            &lane.workspace_id(),
            NewSchedule {
                fleet: &fleet,
                source: Source::Fleet,
                source_key: None,
                cron: SET_FOR_CRON,
                timezone: "UTC",
                message: "check the deploy",
                once: true,
            },
            token,
            UnixMillis::from_millis(ASKED_AT),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("the fixture's own create must be admitted")
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn a_one_off_synced_after_its_moment_is_retired() {
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let claim = CronLane::token();
    let created = one_off(&lane, &claim).await;
    assert_eq!(
        created.fire_at,
        Some(SET_FOR),
        "the create keeps its moment"
    );

    let reconciled = against_live(&lane, url, token)
        .reconcile(
            &created,
            &claim,
            UnixMillis::from_millis(SET_FOR + MINUTE_MS),
        )
        .await
        .expect("a reachable scheduler is not a datastore failure");

    assert!(
        matches!(reconciled, Reconciled::Removed),
        "a one-off past its moment is retired, got {reconciled:?}"
    );
    let left = lane
        .store
        .one(&lane.fleet_id(), &created.schedule_id)
        .await
        .expect("the lane's Postgres must answer");
    assert_eq!(left, None, "and its row is gone");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn a_one_off_synced_before_its_moment_is_registered() {
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let claim = CronLane::token();
    let created = one_off(&lane, &claim).await;

    let reconciled = against_live(&lane, url, token)
        .reconcile(&created, &claim, UnixMillis::from_millis(ASKED_AT))
        .await
        .expect("a reachable scheduler is not a datastore failure");

    assert!(
        matches!(reconciled, Reconciled::Synced(_)),
        "a one-off still ahead of its moment registers, got {reconciled:?}"
    );
}

/// Greptile on #731: a one-off `QStash` already holds keeps its row past its
/// moment, so a delayed callback still finds it.
#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn a_registered_one_off_synced_after_its_moment_is_kept() {
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let claim = CronLane::token();
    let created = one_off(&lane, &claim).await;
    let asked = UnixMillis::from_millis(ASKED_AT);
    lane.store
        .finalize_synced(&created, &claim, Some("qstash-issued-key"), asked)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the creator's own finalize must land");
    let past_moment = UnixMillis::from_millis(SET_FOR + MINUTE_MS);
    let resync = CronLane::token();
    let held = lane
        .store
        .claim_current(&lane.fleet_id(), &created.schedule_id, &resync, past_moment)
        .await
        .expect("the lane's Postgres must answer")
        .expect("a settled schedule is claimable");

    let reconciled = against_live(&lane, url, token)
        .reconcile(&held, &resync, past_moment)
        .await
        .expect("a reachable scheduler is not a datastore failure");

    assert!(
        matches!(reconciled, Reconciled::Synced(_)),
        "a registered one-off is not retired, got {reconciled:?}"
    );
}

/// An edit that moves a one-off sets it for the new moment.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_one_off_moved_by_an_edit_is_set_for_its_new_moment() {
    let lane = CronLane::open().await;
    let claim = CronLane::token();
    let created = one_off(&lane, &claim).await;
    let asked = UnixMillis::from_millis(ASKED_AT);
    lane.store
        .finalize_synced(&created, &claim, None, asked)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the creator's own finalize must land");

    // Five minutes later the same day: 09:10 rather than 09:05.
    let moved = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            &created.schedule_id,
            Change {
                cron: Some("10 9 15 3 *"),
                ..Change::default()
            },
            &CronLane::token(),
            asked,
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("a settled schedule is claimable");

    assert_eq!(moved.fire_at, Some(SET_FOR + 5 * MINUTE_MS));
}
