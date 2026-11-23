//! A guarded write proves its guard on its own transaction, and one whose
//! guard no longer holds writes nothing.
//!
//! The guard here is a stub that answers a fixed verdict; the run a runner's
//! write proves is `afd_fleet::run::write_fence`, tested against live run
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

use self::support::{CronLane, SEED_AT};

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
const fn fleet_made(fleet: &af_core::id::Uuid7) -> NewSchedule<'_> {
    NewSchedule {
        fleet,
        source: Source::Fleet,
        provider_schedule_id: None,
        cron: "0 9 * * 1",
        timezone: "UTC",
        message: "weekly check",
        once: false,
    }
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_create_under_a_guard_that_no_longer_holds_writes_nothing() {
    let lane = CronLane::open().await;
    let fleet = lane.fleet_id();
    let before = lane.count().await;

    let refused = lane
        .store
        .create_guarded(
            &lane.workspace_id(),
            &CronLane::token(),
            fleet_made(&fleet),
            &CronLane::token(),
            SEED_AT,
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
            &CronLane::token(),
            fleet_made(&fleet),
            &CronLane::token(),
            SEED_AT,
            &Verdict(true),
        )
        .await
        .expect("the lane's Postgres must answer");
    assert!(created.is_ok(), "a guard that holds lets the create land");
    assert_eq!(lane.count().await, before + 1);
}

/// A fleet's create keyed by its call: the retry of a call whose create
/// committed but whose answer was lost finds the row the first attempt wrote,
/// claims it for its own push, and writes no second schedule.
#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_keyed_create_repeated_answers_the_row_it_wrote() {
    let lane = CronLane::open().await;
    let fleet = lane.fleet_id();
    let call = CronLane::token();
    let (first_push, retry_push) = (CronLane::token(), CronLane::token());
    let create = async |push: &af_core::id::Uuid7| {
        lane.store
            .create_guarded(
                &lane.workspace_id(),
                &call,
                fleet_made(&fleet),
                push,
                SEED_AT,
                &Verdict(true),
            )
            .await
    };
    let first = create(&first_push)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the first attempt lands");
    lane.store
        .finalize_synced(&first, &first_push, None)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the first attempt's push lets go");

    let retried = create(&retry_push)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the retry answers the first attempt's row");

    assert_eq!(first.schedule_id, call, "the row is keyed by its call");
    assert_eq!(retried.schedule_id, call);
    assert_eq!(
        retried.sync_token,
        Some(retry_push),
        "claimed for the retry's push"
    );
    assert_eq!(lane.count().await, 1, "one call, one schedule");
}

/// A key a person's schedule of the same fleet holds, or another fleet's
/// schedule holds, is refused as a duplicate and touches neither row; a
/// retry whose run no longer holds claims nothing.
#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_keyed_create_never_answers_a_row_it_did_not_write() {
    let lane = CronLane::open().await;
    let other = CronLane::open().await;
    let fleet = lane.fleet_id();
    let person = lane.settled("key-person", "0 9 * * 1").await;
    let theirs = other.settled("key-theirs", "0 9 * * 1").await;
    let create = async |key: &af_core::id::Uuid7, holds: bool| {
        lane.store
            .create_guarded(
                &lane.workspace_id(),
                key,
                fleet_made(&fleet),
                &CronLane::token(),
                SEED_AT,
                &Verdict(holds),
            )
            .await
            .expect("the lane's Postgres must answer")
    };
    let written = lane.count().await;

    assert_eq!(
        create(&person.schedule_id, true).await,
        Err(Refused::DuplicateKey)
    );
    assert_eq!(
        create(&theirs.schedule_id, true).await,
        Err(Refused::DuplicateKey)
    );
    let call = CronLane::token();
    create(&call, true).await.expect("a fresh call lands");
    assert_eq!(create(&call, false).await, Err(Refused::Unheld));

    assert_eq!(lane.count().await, written + 1);
    assert_eq!(
        lane.store
            .one(&fleet, &person.schedule_id)
            .await
            .expect("the lane's Postgres must answer"),
        Some(person),
        "the person's schedule is untouched"
    );
    assert_eq!(other.count().await, 1, "the other fleet keeps its one");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
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
            &Verdict(true),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("a guard that holds is not refused")
        .expect("an unheld row is claimable");
    assert_eq!(claimed.desired_status, DesiredStatus::Paused);
}
