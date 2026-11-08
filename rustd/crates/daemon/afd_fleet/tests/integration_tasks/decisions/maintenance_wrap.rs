//! A full recovery page revisits skipped work before new arrivals can starve it.

use afd_fleet::task::{DecisionMaintenanceCursor, TaskLifecycle as _};
use sqlx::Connection as _;

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, lock_slot},
    maintenance::{due, state},
    maintenance_fairness::{due_after_scan, extra_unissued},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_full_recovery_page_wraps_to_old_contention_before_new_arrivals_can_starve_it() {
    exercise(wrapped).await;
}

async fn wrapped(case: &Case) {
    let other = case.sibling().await;
    let oldest = Decisions::create(case).await;
    let later = Decisions::with_daily_budget(&other, 3_000_000_000).await;
    let balances = (oldest.wallet().await, later.wallet().await);
    let first = extra_unissued(&oldest).await;
    let mut calls = Vec::with_capacity(101);
    for _ in 0..99 {
        calls.push(extra_unissued(&later).await);
    }
    due(&oldest, std::slice::from_ref(&first)).await;
    due(&later, &calls).await;
    let (mut cursor, arrivals) = full_page(&oldest, &later).await;
    calls.extend(arrivals);
    recovered(&oldest, &later, &first, &calls, &mut cursor).await;
    assert_eq!((oldest.wallet().await, later.wallet().await), balances);
    oldest
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
    later
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
    other.held.fixtures.cleanup().await;
}

async fn full_page(
    oldest: &Decisions<'_>,
    later: &Decisions<'_>,
) -> (DecisionMaintenanceCursor, [String; 2]) {
    let mut connection = oldest
        .case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("owned slot holder");
    let mut blocked = connection.begin().await.expect("oldest native owner");
    let _pid = lock_slot(&mut blocked, oldest.case).await;
    let mut cursor = DecisionMaintenanceCursor::default();
    let page = oldest
        .tasks
        .maintain_decisions(&mut cursor)
        .await
        .expect("exact full page");
    assert_eq!(
        (page.scanned, page.expired, page.deferred, page.failed),
        (100, 99, 1, 0)
    );
    let first = extra_unissued(later).await;
    due_after_scan(later, &first).await;
    let second = extra_unissued(later).await;
    due_after_scan(later, &second).await;
    blocked.rollback().await.expect("release the skipped owner");
    (cursor, [first, second])
}

async fn recovered(
    oldest: &Decisions<'_>,
    later: &Decisions<'_>,
    first: &str,
    calls: &[String],
    cursor: &mut DecisionMaintenanceCursor,
) {
    let recovered = oldest
        .tasks
        .maintain_decisions(cursor)
        .await
        .expect("empty seek wraps");
    assert_eq!(
        (
            recovered.scanned,
            recovered.expired,
            recovered.deferred,
            recovered.failed
        ),
        (3, 3, 0, 0)
    );
    let settled = ("not_sent".into(), false, true, Some(0));
    assert_eq!(state(oldest, first).await, settled);
    for call in calls {
        assert_eq!(state(later, call).await, settled);
    }
    drained(oldest, later, cursor).await;
}

async fn drained(
    oldest: &Decisions<'_>,
    later: &Decisions<'_>,
    cursor: &mut DecisionMaintenanceCursor,
) {
    let empty = oldest
        .tasks
        .maintain_decisions(cursor)
        .await
        .expect("all work was recovered");
    assert_eq!(
        (empty.scanned, empty.expired, empty.deferred, empty.failed),
        (0, 0, 0, 0)
    );
    assert_eq!(oldest.counts().await, (1, 1, 0));
    assert_eq!(later.counts().await, (101, 101, 0));
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM runner.decision_calls WHERE original_lease_id IN ($1::uuid,$2::uuid) AND accounted_at IS NULL")
        .bind(oldest.case.held.lease_key.as_str()).bind(later.case.held.lease_key.as_str())
        .fetch_one(&mut *oldest.case.held.fixtures.database.acquire().await.expect("complete retained reservations"))
        .await.expect("nothing remains reserved");
    assert_eq!(remaining, 0);
}
