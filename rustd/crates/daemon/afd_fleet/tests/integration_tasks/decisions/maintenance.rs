//! Abandoned issuance recovers capacity while uncertain paid calls remain owed.

use af_core::id::Uuid7;
use std::time::Duration;

use af_core::error_code;
use af_wire::task::{
    DecisionAssessment, DecisionNotSentCause, DecisionReceipt, DecisionReservationInput,
    DecisionUnavailableCause,
};
use afd_db::test_util::mint_id;
use afd_fleet::task::{DecisionMaintenanceCursor, TaskLifecycle as _};
use serde_json::Value;
use sqlx::Connection as _;
use tokio::time::timeout;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};
use crate::seed::{MODEL, PROVIDER};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn abandoned_calls_release_once_without_freeing_future_or_uncertain_work() {
    exercise(capacity).await;
}

async fn capacity(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let goal = || {
        fixture.reservation(
            DecisionAssessment::Goal {
                action_group_id: mint_id().into(),
            },
            Some(task.clone()),
        )
    };
    let abandoned = goal();
    let expired = unissued(&fixture, &abandoned).await;
    let future = unissued(&fixture, &goal()).await;
    let message = fixture.reservation(DecisionAssessment::Message, Some(task.clone()));
    let source = unissued(&fixture, &message).await;
    let (_, issued) = fixture.reserve(&goal()).await;
    let paid = id(issued
        .get("call_id")
        .and_then(Value::as_str)
        .expect("issued call"));
    let unknown = DecisionReceipt::Unknown {
        cause: DecisionUnavailableCause::Timeout,
    };
    fixture.record(paid.as_str(), unknown).await;
    due(&fixture, &[expired.clone(), source.clone(), paid.clone()]).await;
    let swept = fixture
        .tasks
        .maintain_decisions(&mut DecisionMaintenanceCursor::default())
        .await
        .expect("bounded native recovery");
    assert_eq!((swept.scanned, swept.expired), (2, 2));
    assert!(swept.oldest_overdue_ms >= 1_000);
    let task_id = id(&task.task_id);
    assert_pending_states(&fixture, &expired, &source, &future, &paid, &task_id).await;
    index_bound(&fixture).await;
    replay_and_late_usage(&fixture, &abandoned, &expired, &issued, &task_id).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn assert_pending_states(
    fixture: &Decisions<'_>,
    expired: &Uuid7,
    source: &Uuid7,
    future: &Uuid7,
    paid: &Uuid7,
    task: &Uuid7,
) {
    assert_eq!(
        state(fixture, expired).await,
        ("not_sent".into(), false, true, Some(0))
    );
    assert_eq!(
        state(fixture, source).await,
        ("not_sent".into(), false, true, Some(0))
    );
    assert_eq!(
        state(fixture, future).await,
        ("reserved".into(), false, false, None)
    );
    assert_eq!(
        state(fixture, paid).await,
        ("unknown".into(), true, false, None)
    );
    assert_eq!(budget(fixture, task).await, (40_000_000, 0));
    assert_eq!(fixture.counts().await, (4, 4, 0));
}

async fn replay_and_late_usage(
    fixture: &Decisions<'_>,
    abandoned: &garde::Valid<DecisionReservationInput>,
    expired: &Uuid7,
    issued: &Value,
    task: &Uuid7,
) {
    let again = fixture
        .tasks
        .maintain_decisions(&mut DecisionMaintenanceCursor::default())
        .await
        .expect("idempotent sweep");
    assert_eq!((again.scanned, again.expired), (0, 0));
    let (key, replay) = fixture.reserve(abandoned).await;
    assert!(!key);
    assert_eq!(replay.get("status"), Some(&serde_json::json!("existing")));
    assert_eq!(replay.get("state"), Some(&serde_json::json!("not_sent")));
    let receipt = DecisionReceipt::NotSent {
        cause: DecisionNotSentCause::Preflight,
    };
    assert!(fixture.record(expired.as_str(), receipt).await.replayed);
    let paid = issued
        .get("call_id")
        .and_then(Value::as_str)
        .expect("issued call");
    let settled = fixture.record(paid, known(&request(issued))).await;
    assert_eq!(settled.charged_nanos.as_deref(), Some("3"));
    assert_eq!(budget(fixture, task).await, (20_000_000, 3));
    assert_eq!(fixture.counts().await, (4, 4, 1));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn abandoned_reservations_recover_after_erasure_without_private_recreation() {
    exercise(erased).await;
}

async fn erased(case: &Case) {
    let fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let call = unissued(&fixture, &input).await;
    due(&fixture, std::slice::from_ref(&call)).await;
    sqlx::query("DELETE FROM fleet.fleets WHERE id=$1")
        .bind(&case.held.fleet)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("owner deletion"),
        )
        .await
        .expect("ordinary privacy erasure");
    assert_eq!(fixture.counts().await, (1, 0, 0));
    let swept = fixture
        .tasks
        .maintain_decisions(&mut DecisionMaintenanceCursor::default())
        .await
        .expect("retained financial recovery");
    assert_eq!((swept.scanned, swept.expired), (1, 1));
    assert_eq!(
        state(&fixture, &call).await,
        ("not_sent".into(), false, true, Some(0))
    );
    assert_eq!(fixture.counts().await, (1, 0, 0));
    assert_eq!(case.counts().await, (0, 0, 0));
    let (issued, replay) = fixture.reserve(&input).await;
    assert!(!issued);
    assert_eq!(replay.get("state"), Some(&serde_json::json!("not_sent")));
    assert!(replay.get("recorded").is_none());
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_blocked_recovery_cancels_without_stranding_the_only_connection() {
    exercise(blocked).await;
}

async fn blocked(case: &Case) {
    let fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let call = unissued(&fixture, &input).await;
    due(&fixture, std::slice::from_ref(&call)).await;
    let (database, tasks) = super::timeouts::single_connection(&fixture).await;
    let backing = &case.held.fixtures.database;
    let mut connection = backing.acquire().await.expect("slot blocker");
    let mut blocker = connection.begin().await.expect("owned transaction");
    sqlx::query("LOCK TABLE fleet.fleets IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .expect("block the implicit table lock before NOWAIT rows");
    // The API login's lock bound ends the wait with `55P03`, which reads as
    // a busy lock (`afd_db::login`): the candidate is deferred to the next
    // pass, and the pass returns rather than stopping as an outage.
    let deferred = timeout(
        Duration::from_secs(6),
        tasks.maintain_decisions(&mut DecisionMaintenanceCursor::default()),
    )
    .await
    .expect("the lock bound ends the wait")
    .expect("a blocked candidate defers");
    assert_eq!(
        (deferred.scanned, deferred.deferred, deferred.expired),
        (1, 1, 0)
    );
    assert_eq!(
        state(&fixture, &call).await,
        ("reserved".into(), false, false, None)
    );
    let _cancelled = timeout(
        Duration::from_millis(200),
        tasks.maintain_decisions(&mut DecisionMaintenanceCursor::default()),
    )
    .await
    .expect_err("the caller drops blocked recovery");
    blocker
        .rollback()
        .await
        .expect("release independent slot lock");
    let swept = timeout(
        Duration::from_secs(5),
        tasks.maintain_decisions(&mut DecisionMaintenanceCursor::default()),
    )
    .await
    .expect("one connection remains usable")
    .expect("same native owner recovers");
    assert_eq!((swept.scanned, swept.expired), (1, 1));
    assert_eq!(fixture.counts().await, (1, 1, 0));
    let terminal = ("not_sent".into(), false, true, Some(0));
    assert_eq!(state(&fixture, &call).await, terminal);
    database.close().await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

pub(super) async fn unissued(
    fixture: &Decisions<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Uuid7 {
    let case = fixture.case;
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("owned pricing");
    let removed = sqlx::query("DELETE FROM model.platform_model_prices WHERE platform_model_id IN (SELECT id FROM model.platform_models WHERE provider=$1 AND model_name=$2)")
        .bind(PROVIDER).bind(MODEL).execute(&mut *connection).await.expect("inject missing coding price");
    assert_eq!(removed.rows_affected(), 1);
    drop(connection);
    let fault = fixture
        .tasks
        .reserve_decision(case.lease(), input)
        .await
        .expect_err("native intent commits before funding refuses");
    assert_eq!(fault.code(), error_code::TASK_DECISION_UNAVAILABLE);
    case.held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    sqlx::query_scalar(
        "SELECT id FROM runner.decision_calls WHERE original_lease_id=$1 AND operation_id=$2",
    )
    .bind(&case.held.lease_key)
    .bind(id(&input.operation_id))
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("retained native identity"),
    )
    .await
    .expect("saved physical intent")
}

pub(super) async fn due(fixture: &Decisions<'_>, ids: &[Uuid7]) {
    let changed = sqlx::query("UPDATE runner.decision_calls SET logical_expires_at=clock_timestamp()-interval '1 second' WHERE id=ANY($1)")
        .bind(ids).execute(&mut *fixture.case.held.fixtures.database.acquire().await.expect("deadline fixture"))
        .await.expect("valid elapsed logical deadlines");
    assert_eq!(
        changed.rows_affected(),
        u64::try_from(ids.len()).expect("small owned set")
    );
}

pub(super) async fn state(
    fixture: &Decisions<'_>,
    id: &Uuid7,
) -> (String, bool, bool, Option<i64>) {
    sqlx::query_as("SELECT status, issued_at IS NOT NULL, accounted_at IS NOT NULL, charged_nanos FROM runner.decision_calls WHERE id=$1")
        .bind(id).fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("financial state"))
        .await.expect("retained call")
}

pub(super) async fn budget(fixture: &Decisions<'_>, task: &Uuid7) -> (i64, i64) {
    sqlx::query_as("SELECT reserved_model_nanos, used_model_nanos FROM fleet.tasks WHERE id=$1")
        .bind(task)
        .fetch_one(
            &mut *fixture
                .case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("task budget"),
        )
        .await
        .expect("original task budget owner")
}

async fn index_bound(fixture: &Decisions<'_>) {
    const STATEMENT: &str = const_format::concatcp!(
        "EXPLAIN (FORMAT JSON, COSTS FALSE) ",
        afd_fleet::task::EXPIRED_DECISION_CALLS_STATEMENT
    );
    let mut connection = fixture
        .case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("range plan");
    let mut tx = connection.begin().await.expect("owned planner settings");
    sqlx::query("SET LOCAL enable_seqscan=off")
        .execute(&mut *tx)
        .await
        .expect("inspect index path");
    for after in [None, Some(af_core::clock::now())] {
        let plan: sqlx::types::Json<Value> = sqlx::query_scalar(STATEMENT)
            .bind(100_i64)
            .bind(after.map(jiff_sqlx::Timestamp::from))
            .bind(after.map(|_| &fixture.case.held.lease_key))
            .bind(after.map(jiff_sqlx::Timestamp::from))
            .fetch_one(&mut *tx)
            .await
            .expect("production candidate plan");
        let bound = plan
            .0
            .pointer("/0/Plan/Plans/0/Index Cond")
            .and_then(Value::as_str)
            .expect("deadline and continuation are index ranges");
        assert!(
            bound.contains("logical_expires_at")
                && bound.contains("statement_timestamp()")
                && bound.contains("ROW("),
            "{plan:?}"
        );
    }
    tx.rollback()
        .await
        .expect("restore ordinary planner settings");
}
