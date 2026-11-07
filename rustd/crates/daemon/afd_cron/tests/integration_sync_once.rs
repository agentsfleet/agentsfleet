//! A one-off reconciled after its moment is retired, not registered a year late.
//!
//! A `once` schedule's expression has no year. A create that cannot reach the
//! scheduler leaves the row `failed`, and a later sync pushes it again; past
//! the minute, the next match of that expression is a year away. The row keeps
//! the instant it was set for, counted from its own database stamp, and the
//! reconciler removes a one-off whose instant has passed instead of
//! registering it.
//!
//! Every instant here is the database's. A case that needs a passed minute
//! moves `fire_at` behind the row's `updated_at` (`CronLane::backdate_fire_at`)
//! rather than waiting for a wall clock.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

#[path = "support/live_qstash.rs"]
mod live_qstash;

use std::time::Duration;

use afd_core::clock;
use afd_core::id::Uuid7;
use afd_cron::{Change, DesiredStatus, NewSchedule, Reconciled, Schedule, Source};
use jiff::tz::TimeZone;

use self::live_qstash::{against_live, live};
use self::support::{CronLane, SEED_AT};

/// 09:05 UTC on 15 March: the one-off's minute, whichever year is next.
const SET_FOR_CRON: &str = "5 9 15 3 *";

/// The same day at 09:10, five minutes later.
const MOVED_CRON: &str = "10 9 15 3 *";

/// How far [`MOVED_CRON`] moves the minute.
const MOVED_BY: Duration = Duration::from_mins(5);

/// A one-off for [`SET_FOR_CRON`], claimed by its creator.
async fn one_off(lane: &CronLane, token: &Uuid7) -> Schedule {
    let fleet = lane.fleet_id();
    lane.store
        .create(
            &lane.workspace_id(),
            NewSchedule {
                fleet: &fleet,
                source: Source::Fleet,
                provider_schedule_id: None,
                cron: SET_FOR_CRON,
                timezone: "UTC",
                message: "check the deploy",
                once: true,
            },
            token,
            SEED_AT,
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("the fixture's own create must be admitted")
}

/// `schedule` as the table holds it now.
async fn reread(lane: &CronLane, schedule: &Schedule) -> Schedule {
    lane.store
        .one(&lane.fleet_id(), &schedule.schedule_id)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the schedule is still there")
}

/// Releases the creator's fence, recording `registered` as the upstream id.
async fn settle(lane: &CronLane, created: &Schedule, claim: &Uuid7, registered: Option<&str>) {
    lane.store
        .finalize_synced(created, claim, registered)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the creator's own finalize must land");
}

/// The fire instant is counted from the row's own `created_at`: after it, and
/// on the minute the expression names.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_once_fire_at_follows_the_rows_own_stamp() {
    let lane = CronLane::open().await;
    let created = one_off(&lane, &CronLane::token()).await;

    let fire_at = created
        .fire_at
        .expect("a readable one-off is set for a moment");
    assert!(
        fire_at > created.created_at,
        "{fire_at} after {}",
        created.created_at
    );
    let wall = fire_at.to_zoned(TimeZone::UTC).datetime();
    assert_eq!(
        (wall.month(), wall.day(), wall.hour(), wall.minute()),
        (3, 15, 9, 5),
        "the minute {SET_FOR_CRON} names"
    );
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
    lane.backdate_fire_at(&created.schedule_id).await;
    let held = reread(&lane, &created).await;

    let reconciled = against_live(&lane, url, token)
        .reconcile(&held, &claim)
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
        .reconcile(&created, &claim)
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
    settle(&lane, &created, &claim, Some("qstash-issued-key")).await;
    lane.backdate_fire_at(&created.schedule_id).await;
    let resync = CronLane::token();
    let held = lane
        .store
        .claim_current(&lane.fleet_id(), &created.schedule_id, &resync)
        .await
        .expect("the lane's Postgres must answer")
        .expect("a settled schedule is claimable");
    assert!(
        held.fire_at.is_some_and(|at| at <= held.updated_at),
        "past its moment"
    );

    let reconciled = against_live(&lane, url, token)
        .reconcile(&held, &resync)
        .await
        .expect("a reachable scheduler is not a datastore failure");

    assert!(
        matches!(reconciled, Reconciled::Synced(_)),
        "a registered one-off is not retired, got {reconciled:?}"
    );
}

/// An edit that moves a one-off sets it for the new moment, counted from the
/// edit's own stamp.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_one_off_moved_by_an_edit_is_set_for_its_new_moment() {
    let lane = CronLane::open().await;
    let claim = CronLane::token();
    let created = one_off(&lane, &claim).await;
    settle(&lane, &created, &claim, None).await;

    let moved = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            &created.schedule_id,
            Change {
                cron: Some(MOVED_CRON),
                ..Change::default()
            },
            &CronLane::token(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("a settled schedule is claimable");

    assert_eq!(
        moved.fire_at,
        created.fire_at.map(|at| clock::after(at, MOVED_BY))
    );
    assert!(moved.fire_at.is_some_and(|at| at > moved.updated_at));
}

/// Greptile on #731: a one-off paused before its moment and resumed after it
/// is retired. The pause removed it upstream, so nothing holds its run.
#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn a_one_off_resumed_after_its_moment_is_retired() {
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let scheduler = against_live(&lane, url, token);
    let claim = CronLane::token();
    let created = one_off(&lane, &claim).await;
    settle(&lane, &created, &claim, Some("qstash-issued-key")).await;

    let paused = edit(&lane, &created, DesiredStatus::Paused).await;
    let Reconciled::Synced(paused) = scheduler
        .reconcile(&paused.0, &paused.1)
        .await
        .expect("a reachable scheduler is not a datastore failure")
    else {
        panic!("a pause the scheduler agreed to is synced");
    };
    assert_eq!(
        paused.provider_schedule_id,
        created.schedule_id.as_str(),
        "a paused one-off holds no upstream id"
    );

    lane.backdate_fire_at(&paused.schedule_id).await;
    let resumed = edit(&lane, &paused, DesiredStatus::Active).await;
    let reconciled = scheduler
        .reconcile(&resumed.0, &resumed.1)
        .await
        .expect("a reachable scheduler is not a datastore failure");
    assert!(
        matches!(reconciled, Reconciled::Removed),
        "a one-off resumed after its moment is retired, got {reconciled:?}"
    );
}

/// `schedule` set to `status`, claimed, with the claim's token.
async fn edit(lane: &CronLane, schedule: &Schedule, status: DesiredStatus) -> (Schedule, Uuid7) {
    let claim = CronLane::token();
    let held = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            &schedule.schedule_id,
            Change {
                desired_status: Some(status),
                ..Change::default()
            },
            &claim,
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("a settled schedule is claimable");
    (held, claim)
}
