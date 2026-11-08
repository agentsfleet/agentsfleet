//! A delete whose fence is taken while the scheduler is still being asked.
//!
//! The fence is checked before the push, so this is the window that check
//! cannot close: the removal is already on the wire when another syncer takes
//! the row. The scheduler then agrees, and the row must still not go — the
//! newer syncer owns it, and deleting under it would erase whatever that
//! syncer is about to record.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

#[path = "support/stub_scheduler.rs"]
mod stub_scheduler;

use afd_cron::{Change, DesiredStatus, Reconciled, ScheduleService as Reconciler};

use self::stub_scheduler::StubScheduler;
use self::support::CronLane;

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_delete_overtaken_mid_removal_leaves_the_row_to_its_new_holder() {
    let lane = CronLane::open().await;
    let settled = lane.settled("key-delete-overtaken", "0 3 * * *").await;
    let loser = CronLane::token();
    let deleting = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            &settled.schedule_id,
            Change {
                desired_status: Some(DesiredStatus::Deleting),
                ..Change::default()
            },
            &loser,
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("the schedule is claimable for deletion");
    let mut scheduler = StubScheduler::start();
    let service = Reconciler::new(lane.store.clone(), scheduler.client());
    let winner = CronLane::token();

    let (reconciled, ()) = tokio::join!(service.reconcile(&deleting, &loser), async {
        let request = scheduler.arrived().await;
        assert!(request.starts_with("DELETE /schedules/"), "got {request}");
        // The first syncer stalls past its lease and a second one claims it.
        lane.expire_lease(&deleting.schedule_id).await;
        lane.store
            .claim_current(&lane.fleet_id(), &deleting.schedule_id, &winner)
            .await
            .expect("the lane's Postgres must answer")
            .expect("an expired lease is reclaimable");
        scheduler.answer(200);
    });

    let reconciled = reconciled.expect("a reachable scheduler is not a datastore failure");
    assert!(
        matches!(reconciled, Reconciled::Superseded),
        "the overtaken syncer deletes nothing, got {reconciled:?}"
    );
    let kept = lane
        .store
        .one(&lane.fleet_id(), &deleting.schedule_id)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the row stays for the syncer that holds it");
    assert_eq!(kept.sync_token.as_deref(), Some(winner.as_str()));
    assert_eq!(kept.desired_status, DesiredStatus::Deleting);
}
