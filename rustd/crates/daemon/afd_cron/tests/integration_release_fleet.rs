//! A fleet's delete removes every schedule from the scheduler, then its row,
//! on the transaction that holds the fleet, before the fleet's purge cascades
//! over the table.
//!
//! The cascade reaches rows and never the scheduler, so a row it removed while
//! its registration stood would keep firing at a fleet nobody can find. These
//! cases stand at each answer the scheduler can give: removed, refused, cut
//! off by the deadline, and not asked because another syncer holds the row or
//! the deadline already passed; then at the purge, which refuses a fleet that
//! still holds a schedule.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

#[path = "support/stub_scheduler.rs"]
mod stub_scheduler;

use af_core::error_code;
use af_core::id::Uuid7;
use afd_cron::{DesiredStatus, Schedule, ScheduleService as Reconciler, SyncStatus, upstream_id};

use self::stub_scheduler::StubScheduler;
use self::support::{CronLane, SEED_AT};

/// A reconciler over the lane's store and `scheduler`.
fn reconciler(lane: &CronLane, scheduler: &StubScheduler) -> Reconciler {
    Reconciler::new(lane.store.clone(), std::sync::Arc::new(scheduler.client()))
}

/// The request line a removal of `schedule` sends.
fn removal_of(schedule: &Schedule) -> String {
    format!(
        "DELETE /schedules/{} HTTP/1.1",
        upstream_id(&schedule.schedule_id)
    )
}

/// How long a release may run when the case does not cut it short: the
/// production budget.
fn budget() -> tokio::time::Instant {
    tokio::time::Instant::now() + afd_cron::qstash::CALL_BUDGET
}

/// Opens the transaction a fleet's delete holds: the fleet's row locked
/// `FOR UPDATE`, as `afd_setup`'s `hold_delete` locks it.
async fn hold(lane: &CronLane) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut transaction = sqlx::Transaction::begin(lane.connection().await, None)
        .await
        .expect("the delete transaction opens");
    sqlx::query("SELECT id FROM fleet.fleets WHERE id = $1 FOR UPDATE")
        .bind(&lane.fleet)
        .execute(&mut *transaction)
        .await
        .expect("the fleet row locks");
    transaction
}

/// Releases the lane's fleet under `deadline` on a held transaction, and
/// commits what it removed, as a delete that stops does.
async fn release(
    lane: &CronLane,
    service: &Reconciler,
    deadline: tokio::time::Instant,
) -> afd_cron::Result<()> {
    let mut transaction = hold(lane).await;
    let released = service
        .release_fleet_on(
            &mut transaction,
            &lane.workspace,
            &lane.fleet,
            SEED_AT,
            deadline,
        )
        .await
        .expect("the lane's Postgres answers the release");
    let registered = released.registered();
    drop(released.into_announcement());
    transaction.commit().await.expect("the release commits");
    registered
}

/// Purges the lane's fleet as production does: its row locked, then
/// `delete_locked`, committed only when it succeeds.
async fn purge(lane: &CronLane) -> afd_fleet_lifecycle::Result<()> {
    let mut transaction = hold(lane).await;
    afd_fleet_lifecycle::delete_locked(&mut transaction, &lane.workspace, &lane.fleet).await?;
    transaction.commit().await.expect("the purge commits");
    Ok(())
}

/// Whether the lane's fleet row is still there.
async fn fleet_exists(lane: &CronLane) -> bool {
    let mut connection = lane.connection().await;
    sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM fleet.fleets WHERE id = $1)")
        .bind(&lane.fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("reading the fleet row")
}

/// One schedule's row, read back.
async fn row(lane: &CronLane, schedule: &Uuid7) -> Option<Schedule> {
    lane.store
        .one(&lane.fleet_id(), schedule)
        .await
        .expect("the lane's Postgres must answer")
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_fleet_delete_removes_every_schedule_upstream_and_then_its_rows() {
    let lane = CronLane::open().await;
    let nightly = lane.settled("key-release-nightly", "0 3 * * *").await;
    let weekly = lane.settled("key-release-weekly", "0 4 * * 1").await;
    let mut scheduler = StubScheduler::start();
    scheduler.answer(200);
    scheduler.answer(200);

    release(&lane, &reconciler(&lane, &scheduler), budget())
        .await
        .expect("a scheduler that removes both releases the fleet");

    assert_eq!(
        scheduler.drained(),
        vec![removal_of(&nightly), removal_of(&weekly)],
        "each schedule is removed upstream under the id it was registered with"
    );
    assert_eq!(
        lane.count().await,
        0,
        "each row goes once its removal lands"
    );
    purge(&lane)
        .await
        .expect("a fleet with no schedules purges");
    assert!(!fleet_exists(&lane).await, "the fleet is gone");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_refused_removal_keeps_its_schedule_and_stops_the_purge_until_repeated() {
    let lane = CronLane::open().await;
    let refused = lane.settled("key-release-refused", "0 3 * * *").await;
    let removed = lane.settled("key-release-removed", "0 4 * * 1").await;
    let mut scheduler = StubScheduler::start();
    scheduler.answer(500);
    scheduler.answer(200);
    let service = reconciler(&lane, &scheduler);

    let held = release(&lane, &service, budget())
        .await
        .expect_err("a schedule the scheduler kept stops the release");

    assert_eq!(held.code(), error_code::SCHEDULE_STILL_REGISTERED);
    assert_eq!(held.current_state(), Some("deleting"));
    assert_eq!(
        held.detail(),
        "Fleet schedules still registered; repeat the delete"
    );
    assert_eq!(
        scheduler.drained(),
        vec![removal_of(&refused), removal_of(&removed)],
        "one refusal does not stop the others' removal, and none is sent twice"
    );
    let kept = row(&lane, &refused.schedule_id)
        .await
        .expect("the refused schedule's row stays while it is registered");
    assert_eq!(kept.desired_status, DesiredStatus::Deleting);
    assert_eq!(kept.sync_status, SyncStatus::Failed);
    assert!(kept.last_error.is_some(), "the row says why: {kept:?}");
    assert!(row(&lane, &removed.schedule_id).await.is_none());

    let purged = purge(&lane)
        .await
        .expect_err("a fleet still holding a schedule must not purge");
    assert_eq!(purged.code(), error_code::SCHEDULE_STILL_REGISTERED);
    assert_eq!(purged.current_state(), Some("deleting"));
    assert_eq!(
        purged.detail(),
        held.detail(),
        "the purge and the release tell the caller one sentence"
    );
    assert!(
        fleet_exists(&lane).await,
        "the refused purge changes nothing"
    );
    assert_eq!(
        lane.count().await,
        1,
        "the registered row survives the refusal"
    );

    scheduler.answer(200);
    release(&lane, &service, budget())
        .await
        .expect("the repeated delete removes what is left");
    assert_eq!(scheduler.drained(), vec![removal_of(&refused)]);
    purge(&lane).await.expect("the repeated purge goes through");
    assert!(!fleet_exists(&lane).await);
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_schedule_another_syncer_holds_stops_the_release_without_asking_the_scheduler() {
    let lane = CronLane::open().await;
    // Born held: its creator has not pushed it yet.
    let held = lane.create("key-release-held", "0 3 * * *").await;
    let mut scheduler = StubScheduler::start();

    let refused = release(&lane, &reconciler(&lane, &scheduler), budget())
        .await
        .expect_err("a held schedule cannot be removed yet");

    assert_eq!(refused.code(), error_code::SCHEDULE_STILL_REGISTERED);
    assert_eq!(
        scheduler.drained(),
        Vec::<String>::new(),
        "nothing is sent for a row this caller does not hold"
    );
    let kept = row(&lane, &held.schedule_id)
        .await
        .expect("the held row stays with its holder");
    assert_eq!(kept.sync_token, held.sync_token, "its fence is untouched");
    assert_eq!(kept.desired_status, DesiredStatus::Active);
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_purge_refuses_a_fleet_whose_schedule_was_never_released() {
    let lane = CronLane::open().await;
    let written_late = lane.settled("key-release-late", "0 3 * * *").await;

    let refused = purge(&lane)
        .await
        .expect_err("the cascade must not take a registered schedule");

    assert_eq!(refused.code(), error_code::SCHEDULE_STILL_REGISTERED);
    assert!(fleet_exists(&lane).await);
    assert!(row(&lane, &written_late.schedule_id).await.is_some());
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_fleet_with_no_schedules_releases_without_calling_the_scheduler() {
    let lane = CronLane::open().await;
    let mut scheduler = StubScheduler::start();

    release(&lane, &reconciler(&lane, &scheduler), budget())
        .await
        .expect("nothing to remove is released");

    assert_eq!(scheduler.drained(), Vec::<String>::new());
    purge(&lane).await.expect("the fleet purges");
    assert!(!fleet_exists(&lane).await);
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_removal_cut_off_by_the_deadline_keeps_its_schedule_registered() {
    let lane = CronLane::open().await;
    let slow = lane.settled("key-release-slow", "0 3 * * *").await;
    // Never answered: the removal is still in flight when the deadline falls.
    let mut scheduler = StubScheduler::start();
    let cut_short = tokio::time::Instant::now() + std::time::Duration::from_millis(200);

    let held = release(&lane, &reconciler(&lane, &scheduler), cut_short)
        .await
        .expect_err("a removal the deadline cut off may still be registered");

    assert_eq!(held.code(), error_code::SCHEDULE_STILL_REGISTERED);
    assert_eq!(scheduler.arrived().await, removal_of(&slow));
    let kept = row(&lane, &slow.schedule_id)
        .await
        .expect("the row stays while its removal is unconfirmed");
    assert_eq!(kept.desired_status, DesiredStatus::Deleting);
    assert_eq!(kept.sync_status, SyncStatus::Failed);
    assert_eq!(
        kept.last_error.as_deref(),
        Some(afd_cron::error::detail::UPSTREAM_UNAVAILABLE)
    );
    assert!(kept.sync_token.is_none(), "the fence goes back: {kept:?}");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_release_past_its_deadline_sends_nothing_and_claims_nothing() {
    let lane = CronLane::open().await;
    let settled = lane.settled("key-release-late-start", "0 3 * * *").await;
    let mut scheduler = StubScheduler::start();

    let held = release(
        &lane,
        &reconciler(&lane, &scheduler),
        tokio::time::Instant::now(),
    )
    .await
    .expect_err("no removal starts past the deadline");

    assert_eq!(held.code(), error_code::SCHEDULE_STILL_REGISTERED);
    assert_eq!(scheduler.drained(), Vec::<String>::new());
    let kept = row(&lane, &settled.schedule_id)
        .await
        .expect("the row stays untouched");
    assert_eq!(kept.desired_status, DesiredStatus::Active);
    assert_eq!(kept.resource_version, settled.resource_version);
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_release_of_a_fleet_outside_the_workspace_touches_nothing() {
    let lane = CronLane::open().await;
    let settled = lane.settled("key-release-foreign", "0 3 * * *").await;
    let mut scheduler = StubScheduler::start();
    let mut transaction = hold(&lane).await;

    let released = reconciler(&lane, &scheduler)
        .release_fleet_on(
            &mut transaction,
            &CronLane::token(),
            &lane.fleet,
            SEED_AT,
            budget(),
        )
        .await
        .expect("the lane's Postgres answers the release");

    released
        .registered()
        .expect("a foreign fleet has nothing to release");
    transaction.commit().await.expect("the release commits");
    assert_eq!(scheduler.drained(), Vec::<String>::new());
    let kept = row(&lane, &settled.schedule_id)
        .await
        .expect("the row stays untouched");
    assert_eq!(kept.desired_status, DesiredStatus::Active);
}
