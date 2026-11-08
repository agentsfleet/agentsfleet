//! Blocked and cancelled SQL cannot strand the native lease or its connection.

use std::time::Duration;

use af_core::{error_code, test_util::trace::Capture};
use af_wire::task::{DecisionAssessment, DecisionReservationInput};
use afd_db::{Db, config::DbRole};
use afd_fleet::{
    lease::Leases,
    task::{TaskLifecycle as _, Tasks},
};
use sqlx::Connection as _;
use tokio::time::timeout;

use super::{
    super::fixture::{Case, exercise},
    fixture::Decisions,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn blocked_and_cancelled_decisions_release_connections_and_preserve_native_ownership() {
    let capture = Capture::install();
    exercise(async |case| blocked(case, &capture).await).await;
}

async fn blocked(case: &Case, capture: &Capture) {
    let fixture = Decisions::create(case).await;
    let (database, tasks) = single_connection(&fixture).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("source blocker");
    let mut blocker = connection
        .begin()
        .await
        .expect("source blocker transaction");
    sqlx::query("SELECT id FROM fleet.admissions WHERE id=$1 FOR UPDATE")
        .bind(&case.held.event_id)
        .execute(&mut *blocker)
        .await
        .expect("hold source after slot can be acquired");
    statement_timeout(&tasks, case, &input, capture).await;
    cancellation(&tasks, case, &input, capture, &database).await;
    blocker.rollback().await.expect("release source blocker");
    assert_eq!(fixture.counts().await, (0, 0, 0));
    let result = timeout(
        Duration::from_secs(5),
        tasks.reserve_decision(case.lease(), &input),
    )
    .await
    .expect("original fleet slot is usable after cancellation")
    .expect("same accepted source recovers");
    assert!(result.into_parts().0);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    database.close().await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

pub(super) async fn single_connection(fixture: &Decisions<'_>) -> (Db, Tasks) {
    let case = fixture.case;
    let database = case
        .held
        .fixtures
        .open_with(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_MIN_POOL_SIZE_API", "0"),
            ],
        )
        .await;
    let tasks = Tasks::new(
        Leases::new(
            database.clone(),
            case.held.fixtures.queue().clone(),
            af_crypto::entropy::Entropy::new(),
        ),
        case.admissions.clone(),
        fixture.dependencies.clone(),
    );
    (database, tasks)
}

async fn statement_timeout(
    tasks: &Tasks,
    case: &Case,
    input: &garde::Valid<DecisionReservationInput>,
    capture: &Capture,
) {
    let failure = timeout(
        Duration::from_secs(6),
        tasks.reserve_decision(case.lease(), input),
    )
    .await
    .expect("server deadline bounds source lock wait")
    .expect_err("the source is blocked");
    assert_eq!(failure.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert!(failure.is_datastore_unavailable());
    let event = capture.only("dt_operation_failed");
    assert_eq!(
        (
            event.field("operation"),
            event.field("outcome"),
            event.field("reason")
        ),
        (
            Some("decision_reserve"),
            Some("timeout"),
            Some("statement_deadline")
        )
    );
}

async fn cancellation(
    tasks: &Tasks,
    case: &Case,
    input: &garde::Valid<DecisionReservationInput>,
    capture: &Capture,
    database: &Db,
) {
    assert!(
        timeout(
            Duration::from_millis(200),
            tasks.reserve_decision(case.lease(), input)
        )
        .await
        .is_err(),
        "the caller cancels while the source remains blocked"
    );
    let cancelled = capture.only("dt_operation_cancelled");
    assert_eq!(
        (cancelled.field("operation"), cancelled.field("reason")),
        (Some("decision_reserve"), Some("operation_dropped"))
    );
    let mut fresh = timeout(Duration::from_secs(2), database.acquire())
        .await
        .expect("cancelled call releases the only permit")
        .expect("fresh usable connection");
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&mut *fresh)
            .await
            .expect("socket protocol is healthy"),
        1
    );
    drop(fresh);
}
