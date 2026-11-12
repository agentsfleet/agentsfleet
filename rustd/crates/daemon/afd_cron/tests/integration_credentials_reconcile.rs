//! What a reconcile leaves behind when the `qstash` bag is not usable, and how
//! the row recovers.
//!
//! The bag is read before the push, so a reconcile that cannot read it has sent
//! nothing: the row must say so in this daemon's own words and stay claimable,
//! and the next `:sync` after an operator stores the bag must register it.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

#[path = "support/fire_token.rs"]
mod fire_token;

#[path = "support/stub_scheduler.rs"]
mod stub_scheduler;

#[path = "support/vault_bag.rs"]
mod vault_bag;

use std::sync::Arc;

use afd_cron::error::detail::UPSTREAM_UNAVAILABLE;
use afd_cron::{Reconciled, ScheduleService as Reconciler, SyncStatus, upstream_id};

use self::stub_scheduler::StubScheduler;
use self::support::CronLane;
use self::vault_bag::{SEALING_KEY, bag, scheduler_over, store, vault_under};

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_row_failed_by_a_missing_bag_syncs_once_the_bag_is_stored() {
    let lane = CronLane::open().await;
    let mut stub = StubScheduler::start();
    let vault = vault_under(&lane, SEALING_KEY);
    let admin = lane.workspace_id();
    let service = Reconciler::new(lane.store.clone(), Arc::new(scheduler_over(&vault, &admin)));
    let created = lane.settled("key-no-bag", "0 6 * * *").await;
    let claim = CronLane::token();
    let held = lane
        .store
        .claim_current(&lane.fleet_id(), &created.schedule_id, &claim)
        .await
        .expect("the lane's Postgres must answer")
        .expect("an unheld schedule is claimable");

    let refused = service
        .reconcile(&held, &claim)
        .await
        .expect("an unusable bag is recorded on the row, never raised");

    let Reconciled::Failed(failed) = refused else {
        panic!("a reconcile with no bag leaves the row failed, got {refused:?}");
    };
    assert_eq!(failed.sync_status, SyncStatus::Failed);
    assert_eq!(
        failed.last_error.as_deref(),
        Some(UPSTREAM_UNAVAILABLE),
        "the row carries this daemon's sentence, naming no field and no value"
    );
    assert!(stub.drained().is_empty(), "no request left the process");

    // An operator stores the bag; the next sync claims the same row again.
    store(&vault, &admin, &bag("tok", stub.base(), "cur", "nxt")).await;
    stub.answer_with(
        200,
        &format!(
            r#"{{"scheduleId":"{}"}}"#,
            upstream_id(&created.schedule_id)
        ),
    );
    let retry = CronLane::token();
    let reclaimed = lane
        .store
        .claim_current(&lane.fleet_id(), &created.schedule_id, &retry)
        .await
        .expect("the lane's Postgres must answer")
        .expect("a failed row is released, so the next sync can claim it");
    let synced = service
        .reconcile(&reclaimed, &retry)
        .await
        .expect("a stored bag is a reachable scheduler");

    let Reconciled::Synced(row) = synced else {
        panic!("the stored bag lets the next sync register the schedule, got {synced:?}");
    };
    assert_eq!(row.sync_status, SyncStatus::Synced);
    assert!(row.last_error.is_none(), "a push that landed clears why");
    assert_eq!(
        stub.authorizations(),
        [Some("Bearer tok".to_owned())],
        "and went out under the stored bag's bearer"
    );
}
