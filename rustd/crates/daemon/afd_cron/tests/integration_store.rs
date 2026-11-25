//! The schedule table's reads, its create, and the three bounds it refuses at.
//!
//! # Refusals are not errors here, and the split is load-bearing
//!
//! `create` answers `Ok(Err(Refused))` for a bound an operator hit and
//! `Err(Error)` for something this daemon got wrong. A caller renders the first
//! as a 4xx naming what to change and the second as a 5xx; collapsing them
//! would make a person's own mistake read as an outage, and an outage read as
//! their mistake.
//!
//! # Every assertion is scoped to this lane's own fleet
//!
//! Postgres is shared across the lane, so a count or a list over the whole
//! table would race whatever else is running. Each fixture mints its own
//! workspace and fleet, and every read below is filtered by them.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_cron::model::MAX_SCHEDULES_PER_FLEET;
use afd_cron::{DesiredStatus, Refused, Source, SyncStatus};
use sqlx::Connection as _;

use self::support::{CronLane, SEED_AT};

/// An expression every fixture registers under.
const NIGHTLY: &str = "0 3 * * *";

/// How long the race waits for both creators to queue.
const QUEUE_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

/// How many backends wait behind `$1`, directly or behind one that does.
const QUEUED_BEHIND: &str = "WITH RECURSIVE behind(pid) AS ( \
    SELECT a.pid FROM pg_stat_activity a WHERE $1 = ANY(pg_blocking_pids(a.pid)) \
    UNION \
    SELECT a.pid FROM pg_stat_activity a JOIN behind b ON b.pid = ANY(pg_blocking_pids(a.pid))) \
    SELECT count(*) FROM behind";

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_created_schedule_reads_back_as_it_was_written() {
    let lane = CronLane::open().await;
    let created = lane.create("key-readback", NIGHTLY).await;

    let read = lane
        .store
        .one(&lane.fleet_id(), &created.schedule_id)
        .await
        .expect("the lane answers")
        .expect("a created schedule is readable");

    assert_eq!(read, created, "the create's answer is the stored row");
    assert_eq!(read.cron, NIGHTLY);
    assert_eq!(read.source, Source::Api);
    assert_eq!(read.fleet_id, lane.fleet_id());
}

/// A new schedule is `Active` intent and `Syncing` observation, never `Synced`.
///
/// The two halves are allowed to disagree and here they must: the row exists,
/// the external scheduler has not been told yet, and a create that claimed
/// `Synced` would make the reconcile skip a schedule that was never registered.
#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_new_schedule_wants_to_fire_and_admits_upstream_does_not_know_yet() {
    let lane = CronLane::open().await;
    let created = lane.create("key-initial-state", NIGHTLY).await;

    assert_eq!(created.desired_status, DesiredStatus::Active);
    assert_eq!(created.sync_status, SyncStatus::Syncing);
    assert!(
        created.last_error.is_none(),
        "a new schedule has no failure behind it"
    );
}

/// Version ONE, never zero, and the column's own CHECK agrees.
///
/// Zero would make "never synced" and "synced at version zero" the same
/// state to a finalize, which is the one comparison the fence turns on.
#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_new_schedule_starts_at_version_one() {
    let lane = CronLane::open().await;
    let created = lane.create("key-version", NIGHTLY).await;

    assert_eq!(created.resource_version, 1);
}

/// A create is fenced from birth: it lands already claimed by its creator.
#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_new_schedule_is_already_held_by_the_caller_that_will_push_it() {
    let lane = CronLane::open().await;
    let created = lane.create("key-born-held", NIGHTLY).await;

    assert!(
        created.sync_token.is_some(),
        "the creator holds it, or a syncer could push a schedule the creating \
         request has not finished writing"
    );
    assert!(created.sync_run_expires_at.is_some());
}

// ── The three bounds ─────────────────────────────────────────────────────────

/// A fleet outside the proven workspace answers exactly as a missing one.
///
/// Telling them apart would confirm a fleet id across a workspace boundary — a
/// caller could enumerate other tenants' fleets by watching which id earns a
/// different refusal.
#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_fleet_outside_the_proven_workspace_is_refused_as_no_such_fleet() {
    let lane = CronLane::open().await;
    let other = CronLane::open().await;
    let foreign_fleet = other.fleet_id();

    let refused = lane
        .store
        .create(
            &lane.workspace_id(),
            afd_cron::NewSchedule {
                fleet: &foreign_fleet,
                source: Source::Api,
                provider_schedule_id: Some("key-foreign"),
                cron: NIGHTLY,
                timezone: "UTC",
                message: "run the nightly repair",
                once: false,
            },
            &CronLane::token(),
            SEED_AT,
        )
        .await
        .expect("the lane answers");

    assert_eq!(refused, Err(Refused::NoSuchFleet));
    assert_eq!(
        other.count().await,
        0,
        "nothing may be written to a fleet the caller was not proven on"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn the_same_upstream_key_twice_on_one_fleet_is_refused() {
    let lane = CronLane::open().await;
    lane.create("key-once", NIGHTLY).await;

    assert_eq!(
        lane.try_create("key-once", "0 4 * * *").await,
        Err(Refused::DuplicateKey),
        "the key is what a signed fire resolves back to; two rows under one key \
         would make a fire ambiguous"
    );
    assert_eq!(lane.count().await, 1, "the refused create wrote nothing");
}

/// The key is unique PER FLEET, so two fleets may each hold the same one.
#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn the_same_upstream_key_on_two_different_fleets_is_admitted() {
    let lane = CronLane::open().await;
    let other = CronLane::open().await;

    lane.create("key-shared", NIGHTLY).await;
    other.create("key-shared", NIGHTLY).await;

    assert_eq!(lane.count().await, 1);
    assert_eq!(other.count().await, 1);
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_fleet_at_its_ceiling_refuses_the_next_schedule() {
    let lane = CronLane::open().await;
    for index in 0..MAX_SCHEDULES_PER_FLEET {
        lane.create(&format!("key-{index}"), NIGHTLY).await;
    }
    assert_eq!(
        lane.count().await,
        i64::try_from(MAX_SCHEDULES_PER_FLEET).expect("the ceiling fits in an i64")
    );

    assert_eq!(
        lane.try_create("key-one-too-many", NIGHTLY).await,
        Err(Refused::TooMany)
    );
    assert_eq!(
        lane.count().await,
        i64::try_from(MAX_SCHEDULES_PER_FLEET).expect("the ceiling fits in an i64"),
        "the refused create must not have written past the ceiling"
    );
}

/// Two creators for the last place under the ceiling, both started while a
/// writer holds the fleet row: the fleet's writer turn lets one count and
/// insert before the other counts, so one is admitted, the other is refused
/// as over the ceiling, and the fleet holds exactly the ceiling. A create that
/// counted before taking its turn would count 31 twice behind the holder and
/// leave 33.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the lane's Postgres and NATS"]
async fn two_creators_at_the_cap_admit_one() {
    let lane = CronLane::open().await;
    for index in 1..MAX_SCHEDULES_PER_FLEET {
        lane.create(&format!("key-{index}"), NIGHTLY).await;
    }
    let mut observer = lane.connection().await;
    let mut connection = lane.connection().await;
    let mut held = connection.begin().await.expect("the holder opens");
    let holder: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM fleet.fleets WHERE id = $1 FOR UPDATE")
            .bind(&lane.fleet)
            .fetch_one(&mut *held)
            .await
            .expect("the holder takes the fleet row");
    let release = async {
        tokio::time::timeout(QUEUE_LIMIT, async {
            while sqlx::query_scalar::<_, i64>(QUEUED_BEHIND)
                .bind(holder)
                .fetch_one(&mut *observer)
                .await
                .expect("the lock graph reads")
                < 2
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("both creators queue behind the holder");
        held.rollback().await.expect("the holder lets go");
    };

    let (first, second, ()) = tokio::join!(
        lane.try_create("key-first", NIGHTLY),
        lane.try_create("key-second", NIGHTLY),
        release
    );

    let answers = [&first, &second];
    assert_eq!(
        answers.iter().filter(|answer| answer.is_ok()).count(),
        1,
        "{first:?} / {second:?}"
    );
    assert!(
        answers
            .iter()
            .any(|answer| matches!(answer, Err(Refused::TooMany))),
        "the other is over the ceiling: {first:?} / {second:?}"
    );
    assert_eq!(
        lane.count().await,
        i64::try_from(MAX_SCHEDULES_PER_FLEET).expect("the ceiling fits in an i64")
    );
}
