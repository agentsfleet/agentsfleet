//! Blocked and cancelled SQL cannot strand the native run or its connection.

use std::time::Duration;

use af_core::{error_code, test_util::trace::Capture};
use af_wire::durable_task::{DecisionAssessment, DecisionReservationInput};
use afd_db::{Db, config::DbRole};
use afd_fleet::{
    durable_task::{DurableTaskLifecycle as _, DurableTasks},
    run::Runs,
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
    sqlx::query("SELECT id FROM thread.turns WHERE id=$1 FOR UPDATE")
        .bind(&case.held.turn_id)
        .execute(&mut *blocker)
        .await
        .expect("hold source after thread can be acquired");
    statement_timeout(&tasks, case, &input, capture).await;
    cancellation(&tasks, case, &input, capture, &database).await;
    assert_eq!(
        fixture.counts().await,
        (0, 0, 0),
        "neither waiter saved a call"
    );
    blocker.rollback().await.expect("release source blocker");
    // The cancelled request's issue statement was already with Postgres. It is one
    // autocommitted statement, so it issues its call once the lock frees, though nobody
    // is left to read the key; the operation's retry reads that call back.
    abandoned_statement_ends(case).await;
    assert_eq!(fixture.counts().await, (1, 1, 0));
    let (issued, body) = timeout(
        Duration::from_secs(5),
        tasks.reserve_decision(case.run(), &input),
    )
    .await
    .expect("original fleet thread is usable after cancellation")
    .expect("the same operation replays")
    .into_parts();
    assert!(!issued, "the retry gets no second key");
    let body: serde_json::Value = serde_json::from_str(&body).expect("a reservation answer");
    assert_eq!(
        (body.get("status"), body.get("state")),
        (
            Some(&serde_json::json!("existing")),
            Some(&serde_json::json!("issued"))
        )
    );
    assert_eq!(fixture.counts().await, (1, 1, 0));
    database.close().await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

/// Waits until no backend is still running an issue statement, named by how
/// `decision/sql/issue.sql` opens.
async fn abandoned_statement_ends(case: &Case) {
    let mut probe = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    timeout(Duration::from_secs(5), async {
        loop {
            let running: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE state <> 'idle' \
                 AND query LIKE '-- A decision reservation''s one locking statement%')",
            )
            .fetch_one(&mut *probe)
            .await
            .expect("inspect the abandoned statement");
            if !running {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the abandoned statement ends once its lock frees");
}

pub(super) async fn single_connection(fixture: &Decisions<'_>) -> (Db, DurableTasks) {
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
    let tasks = DurableTasks::new(
        Runs::new(
            database.clone(),
            case.held.fixtures.bus().clone(),
            af_crypto::entropy::Entropy::new(),
        ),
        case.turns.clone(),
        fixture.dependencies.clone(),
    );
    (database, tasks)
}

async fn statement_timeout(
    tasks: &DurableTasks,
    case: &Case,
    input: &garde::Valid<DecisionReservationInput>,
    capture: &Capture,
) {
    let failure = timeout(
        Duration::from_secs(6),
        tasks.reserve_decision(case.run(), input),
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
    tasks: &DurableTasks,
    case: &Case,
    input: &garde::Valid<DecisionReservationInput>,
    capture: &Capture,
    database: &Db,
) {
    assert!(
        timeout(
            Duration::from_millis(200),
            tasks.reserve_decision(case.run(), input)
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
