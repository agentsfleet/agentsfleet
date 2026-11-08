//! Native intents beyond a whole busy page remain reachable on later passes.

use afd_fleet::task::{DecisionMaintenanceCursor, TaskLifecycle as _};
use serde_json::json;
use sqlx::Connection as _;

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, lock_slot, policy},
    maintenance::{due, state},
    maintenance_fairness::{due_after_scan, extra_unissued},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn recovery_seeks_past_a_full_busy_page_and_wraps_to_retry_every_skipped_reservation() {
    exercise(backlog).await;
}

async fn backlog(case: &Case) {
    let oldest = Decisions::create(case).await;
    let calls = busy_backlog(&oldest).await;
    let other = case.sibling().await;
    let later = Decisions::create(&other).await;
    let reachable = extra_unissued(&later).await;
    due(&oldest, &calls).await;
    due(&later, std::slice::from_ref(&reachable)).await;
    let (mut cursor, arrivals) = busy_passes(&oldest, &later, &reachable).await;
    retried(&oldest, &mut cursor, &calls).await;
    for call in arrivals {
        assert_eq!(
            state(&later, &call).await,
            ("not_sent".into(), false, true, Some(0))
        );
    }
    assert_eq!(later.counts().await, (3, 3, 0));
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

async fn busy_passes(
    fixture: &Decisions<'_>,
    later: &Decisions<'_>,
    reachable: &str,
) -> (DecisionMaintenanceCursor, [String; 2]) {
    let database = &fixture.case.held.fixtures.database;
    let mut connection = database.acquire().await.expect("busy oldest fleet");
    let mut blocker = connection.begin().await.expect("native slot owner");
    let _pid = lock_slot(&mut blocker, fixture.case).await;
    let mut cursor = DecisionMaintenanceCursor::default();
    let first = fixture
        .tasks
        .maintain_decisions(&mut cursor)
        .await
        .expect("bounded full page");
    assert_eq!(
        (first.scanned, first.expired, first.deferred, first.failed),
        (100, 0, 100, 0)
    );
    let arrivals = new_arrivals(later).await;
    let next = fixture
        .tasks
        .maintain_decisions(&mut cursor)
        .await
        .expect("seek after whole busy page");
    assert_eq!(
        (next.scanned, next.expired, next.deferred, next.failed),
        (2, 1, 1, 0)
    );
    assert_eq!(fixture.counts().await, (101, 101, 0));
    assert_eq!(later.counts().await, (3, 3, 0));
    for call in &arrivals {
        assert_eq!(
            state(later, call).await,
            ("reserved".into(), false, false, None)
        );
    }
    assert_eq!(
        state(later, reachable).await,
        ("not_sent".into(), false, true, Some(0))
    );
    blocker
        .rollback()
        .await
        .expect("release all skipped native work");
    (cursor, arrivals)
}

async fn new_arrivals(fixture: &Decisions<'_>) -> [String; 2] {
    let first = extra_unissued(fixture).await;
    due_after_scan(fixture, &first).await;
    let second = extra_unissued(fixture).await;
    due_after_scan(fixture, &second).await;
    [first, second]
}

async fn busy_backlog(fixture: &Decisions<'_>) -> Vec<String> {
    let config = json!({"name":"pull-request-repair", "x-agentsfleet":{"tools":[], "triggers":[],
        "budget":{"daily_nanos":"1000000000000"}, "decision_policy":policy()}});
    fixture
        .case
        .held
        .fixtures
        .native_config(&fixture.case.held.fleet, &config.to_string())
        .await;
    let mut calls = Vec::with_capacity(101);
    for _ in 0..101 {
        calls.push(extra_unissued(fixture).await);
    }
    assert_eq!(fixture.counts().await, (101, 101, 0));
    calls
}

async fn retried(
    fixture: &Decisions<'_>,
    cursor: &mut DecisionMaintenanceCursor,
    calls: &[String],
) {
    let first = fixture
        .tasks
        .maintain_decisions(cursor)
        .await
        .expect("wrap to deferred page");
    let second = fixture
        .tasks
        .maintain_decisions(cursor)
        .await
        .expect("last deferred candidate");
    assert_eq!(
        (first.scanned, first.expired, first.deferred, first.failed),
        (100, 100, 0, 0)
    );
    assert_eq!(
        (
            second.scanned,
            second.expired,
            second.deferred,
            second.failed
        ),
        (3, 3, 0, 0)
    );
    for call in calls {
        assert_eq!(
            state(fixture, call).await,
            ("not_sent".into(), false, true, Some(0))
        );
    }
    let empty = fixture
        .tasks
        .maintain_decisions(cursor)
        .await
        .expect("no repeated refunds");
    assert_eq!((empty.scanned, empty.expired, empty.failed), (0, 0, 0));
    assert_eq!(fixture.counts().await, (101, 101, 0));
}
