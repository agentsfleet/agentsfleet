//! Cancelled recovery retains its cursor and releases the sole SQL pool permit.

use std::time::Duration;

use afd_fleet::task::{DecisionMaintenanceCursor, TaskLifecycle as _, Tasks};
use afd_wire::task::DecisionAssessment;
use sqlx::Connection as _;
use tokio::time::timeout;

use super::{
    super::fixture::{Case, exercise},
    fixture::Decisions,
    maintenance::{due, state, unissued},
    maintenance_fairness::extra_unissued,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn cancellation_retains_scan_progress_and_the_only_connection_for_later_work() {
    exercise(cancelled).await;
}

async fn cancelled(case: &Case) {
    let fixture = Decisions::create(case).await;
    let first = unissued(
        &fixture,
        &fixture.reservation(DecisionAssessment::Message, None),
    )
    .await;
    let second = extra_unissued(&fixture).await;
    due(&fixture, &[first.clone(), second.clone()]).await;
    let (database, tasks) = super::timeouts::single_connection(&fixture).await;
    let mut cursor = DecisionMaintenanceCursor::default();
    cancel_at_parent(&fixture, &tasks, &mut cursor, &first).await;
    let later = timeout(
        Duration::from_secs(5),
        tasks.maintain_decisions(&mut cursor),
    )
    .await
    .expect("pool permit is reusable")
    .expect("continue past cancelled row");
    assert_eq!((later.scanned, later.expired, later.failed), (1, 1, 0));
    assert_eq!(
        state(&fixture, &first).await,
        ("reserved".into(), false, false, None)
    );
    assert_eq!(
        state(&fixture, &second).await,
        ("not_sent".into(), false, true, Some(0))
    );
    let wrapped = tasks
        .maintain_decisions(&mut cursor)
        .await
        .expect("wrap retries cancelled row");
    assert_eq!(
        (wrapped.scanned, wrapped.expired, wrapped.failed),
        (1, 1, 0)
    );
    assert_eq!(
        state(&fixture, &first).await,
        ("not_sent".into(), false, true, Some(0))
    );
    assert_eq!(fixture.counts().await, (2, 2, 0));
    database.close().await;
    fixture.model.finish().await;
}

async fn cancel_at_parent(
    fixture: &Decisions<'_>,
    tasks: &Tasks,
    cursor: &mut DecisionMaintenanceCursor,
    first: &str,
) {
    let mut connection = fixture
        .case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("parent table blocker");
    let mut blocker = connection
        .begin()
        .await
        .expect("independent lock transaction");
    sqlx::query("LOCK TABLE fleet.fleets IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .expect("hold implicit table lock before NOWAIT rows");
    let _cancelled = timeout(Duration::from_millis(200), tasks.maintain_decisions(cursor))
        .await
        .expect_err("cancel after the first candidate advanced");
    assert_eq!(fixture.counts().await, (2, 2, 0));
    assert_eq!(
        state(fixture, first).await,
        ("reserved".into(), false, false, None)
    );
    blocker
        .rollback()
        .await
        .expect("release independent table lock");
}
