//! A guarded write proves its guard on its own transaction, and one whose
//! guard no longer holds writes nothing.
//!
//! The guard here is a stub that answers a fixed verdict; the lease a runner's
//! write proves is `afd_fleet::lease::write_fence`, tested against live lease
//! rows in `agentsfleetd`'s `integration_runner_schedules_fence`.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use std::pin::Pin;

use afd_cron::{Change, DesiredStatus, NewSchedule, Refused, Source};
use afd_db::Precondition;
use sqlx::PgConnection;

use self::support::CronLane;

/// A guard that answers its verdict every time it is asked.
struct Verdict(bool);

impl Precondition for Verdict {
    fn holds<'c>(
        &'c self,
        _connection: &'c mut PgConnection,
    ) -> Pin<Box<dyn Future<Output = sqlx::Result<bool>> + Send + 'c>> {
        Box::pin(std::future::ready(Ok(self.0)))
    }
}

/// A fleet's new schedule on the lane's fleet.
fn fleet_made(fleet: &afd_core::id::Uuid7) -> NewSchedule<'_> {
    NewSchedule {
        fleet,
        source: Source::Fleet,
        source_key: None,
        cron: "0 9 * * 1",
        timezone: "UTC",
        message: "weekly check",
        once: false,
    }
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_create_under_a_guard_that_no_longer_holds_writes_nothing() {
    let lane = CronLane::open().await;
    let fleet = lane.fleet_id();
    let before = lane.count().await;

    let refused = lane
        .store
        .create_guarded(
            &lane.workspace_id(),
            fleet_made(&fleet),
            &CronLane::token(),
            CronLane::now(),
            &Verdict(false),
        )
        .await
        .expect("the lane's Postgres must answer");
    assert_eq!(refused.err(), Some(Refused::Unheld));
    assert_eq!(lane.count().await, before, "no row was written");

    let created = lane
        .store
        .create_guarded(
            &lane.workspace_id(),
            fleet_made(&fleet),
            &CronLane::token(),
            CronLane::now(),
            &Verdict(true),
        )
        .await
        .expect("the lane's Postgres must answer");
    assert!(created.is_ok(), "a guard that holds lets the create land");
    assert_eq!(lane.count().await, before + 1);
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_change_under_a_guard_that_no_longer_holds_writes_nothing() {
    let lane = CronLane::open().await;
    let settled = lane.settled("key-guarded-change", "0 9 * * 1").await;
    let pause = Change {
        desired_status: Some(DesiredStatus::Paused),
        ..Change::default()
    };

    let refused = lane
        .store
        .claim_change_guarded(
            &lane.fleet_id(),
            &settled.schedule_id,
            pause,
            &CronLane::token(),
            CronLane::now(),
            &Verdict(false),
        )
        .await
        .expect("the lane's Postgres must answer");
    assert_eq!(refused.err(), Some(Refused::Unheld));
    let unchanged = lane
        .store
        .one(&lane.fleet_id(), &settled.schedule_id)
        .await
        .expect("the lane's Postgres must answer");
    assert_eq!(unchanged, Some(settled.clone()), "the row is as it was");

    let claimed = lane
        .store
        .claim_change_guarded(
            &lane.fleet_id(),
            &settled.schedule_id,
            pause,
            &CronLane::token(),
            CronLane::now(),
            &Verdict(true),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("a guard that holds is not refused")
        .expect("an unheld row is claimable");
    assert_eq!(claimed.desired_status, DesiredStatus::Paused);
}
