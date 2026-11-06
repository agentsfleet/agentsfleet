//! Recovery reaches other fleets while the oldest candidate remains busy.

use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afd_fleet::task::{DecisionMaintenanceCursor, TaskLifecycle as _};
use afd_wire::task::{DecisionAssessment, DecisionReservationInput};
use sqlx::Connection as _;
use tokio::time::timeout;

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, lock_slot},
    maintenance::{due, state, unissued},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_held_oldest_slot_does_not_starve_a_later_fleet() {
    exercise(later_fleet).await;
}

async fn later_fleet(case: &Case) {
    let other = case.sibling().await;
    let oldest = Decisions::create(case).await;
    let later = Decisions::create(&other).await;
    let first = unissued(
        &oldest,
        &oldest.reservation(DecisionAssessment::Message, None),
    )
    .await;
    let second = unissued(
        &later,
        &later.reservation(DecisionAssessment::Message, None),
    )
    .await;
    due(&oldest, std::slice::from_ref(&first)).await;
    due(&later, std::slice::from_ref(&second)).await;
    let mut cursor = DecisionMaintenanceCursor::default();
    let arriving = held_pass(&oldest, &later, &mut cursor).await;
    assert_eq!(
        state(&oldest, &first).await,
        ("reserved".into(), false, false, None)
    );
    assert_eq!(
        state(&later, &second).await,
        ("not_sent".into(), false, true, Some(0))
    );
    assert_eq!(oldest.counts().await, (1, 1, 0));
    assert_eq!(later.counts().await, (2, 2, 0));
    retried_pair(&oldest, &later, &first, &arriving, &mut cursor).await;
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

async fn held_pass(
    fixture: &Decisions<'_>,
    later: &Decisions<'_>,
    cursor: &mut afd_fleet::task::DecisionMaintenanceCursor,
) -> String {
    let mut connection = fixture
        .case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("busy fleet");
    let mut blocker = connection
        .begin()
        .await
        .expect("in-flight native operation");
    let _pid = lock_slot(&mut blocker, fixture.case).await;
    let capture = Capture::install();
    let swept = timeout(
        Duration::from_secs(1),
        fixture.tasks.maintain_decisions(cursor),
    )
    .await
    .expect("a held oldest fleet cannot delay unrelated recovery")
    .expect("complete bounded pass");
    assert_eq!(
        (swept.scanned, swept.expired, swept.deferred, swept.failed),
        (2, 1, 1, 0)
    );
    healthy_diagnostics(&capture);
    let arriving = extra_unissued(later).await;
    due_after_scan(later, &arriving).await;
    blocker
        .rollback()
        .await
        .expect("release still-held oldest slot");
    arriving
}

async fn retried_pair(
    oldest: &Decisions<'_>,
    later: &Decisions<'_>,
    first: &str,
    arriving: &str,
    cursor: &mut DecisionMaintenanceCursor,
) {
    assert_eq!(
        state(later, arriving).await,
        ("reserved".into(), false, false, None)
    );
    let retried = oldest
        .tasks
        .maintain_decisions(cursor)
        .await
        .expect("new arrivals cannot postpone old work");
    assert_eq!(
        (
            retried.scanned,
            retried.expired,
            retried.deferred,
            retried.failed
        ),
        (2, 2, 0, 0)
    );
    assert_eq!(
        state(oldest, first).await,
        ("not_sent".into(), false, true, Some(0))
    );
    assert_eq!(
        state(later, arriving).await,
        ("not_sent".into(), false, true, Some(0))
    );
    assert_eq!(later.counts().await, (2, 2, 0));
}

fn healthy_diagnostics(capture: &Capture) {
    let event = capture.only("dt_decision_maintenance_completed");
    assert_eq!(event.level, tracing::Level::INFO);
    assert_eq!(
        (
            event.field("scanned"),
            event.field("expired"),
            event.field("deferred"),
            event.field("failed")
        ),
        (Some("2"), Some("1"), Some("1"), Some("0"))
    );
    let deferred = capture.only("dt_decision_recovery_deferred");
    assert_eq!(deferred.level, tracing::Level::DEBUG);
    assert_eq!(
        (deferred.field("outcome"), deferred.field("reason")),
        (Some("contended"), Some("financial_lock_busy"))
    );
    assert!(capture.events().iter().all(|event| {
        event.fields.values().all(|value| {
            !value.contains("native-decision-fixture-")
                && !value.contains("Does the unchanged regression")
        })
    }));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn malformed_oldest_state_is_retained_while_later_work_recovers_and_wrap_retries_repair() {
    exercise(malformed).await;
}

async fn malformed(case: &Case) {
    let fixture = Decisions::create(case).await;
    let first = unissued(
        &fixture,
        &fixture.reservation(DecisionAssessment::Message, None),
    )
    .await;
    let second = extra_unissued(&fixture).await;
    due(&fixture, &[first.clone(), second.clone()]).await;
    replace_profile(&fixture, &first, &serde_json::json!({})).await;
    let capture = Capture::install();
    let mut cursor = DecisionMaintenanceCursor::default();
    let swept = fixture
        .tasks
        .maintain_decisions(&mut cursor)
        .await
        .expect("isolate bad row");
    assert_eq!(
        (swept.scanned, swept.expired, swept.deferred, swept.failed),
        (2, 1, 0, 1)
    );
    assert_eq!(
        state(&fixture, &first).await,
        ("reserved".into(), false, false, None)
    );
    let terminal = ("not_sent".into(), false, true, Some(0));
    assert_eq!(state(&fixture, &second).await, terminal);
    assert_eq!(fixture.counts().await, (2, 2, 0));
    failed_diagnostics(&capture);
    replace_profile(
        &fixture,
        &first,
        &serde_json::to_value(&fixture.assignment).expect("original profile"),
    )
    .await;
    let repaired = fixture
        .tasks
        .maintain_decisions(&mut cursor)
        .await
        .expect("wrap revisits repaired row");
    assert_eq!(
        (repaired.scanned, repaired.expired, repaired.failed),
        (1, 1, 0)
    );
    assert_eq!(state(&fixture, &first).await, terminal);
    assert_eq!(fixture.counts().await, (2, 2, 0));
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn replace_profile(fixture: &Decisions<'_>, call: &str, value: &serde_json::Value) {
    sqlx::query("UPDATE runner.decision_calls SET profile_snapshot=$2 WHERE id=$1::uuid")
        .bind(call)
        .bind(sqlx::types::Json(value))
        .execute(
            &mut *fixture
                .case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("owned snapshot"),
        )
        .await
        .expect("replace owned stored profile");
}

fn failed_diagnostics(capture: &Capture) {
    let event = capture.only("dt_decision_maintenance_completed");
    assert_eq!(event.level, tracing::Level::WARN);
    assert_eq!(
        event.field("error_code"),
        Some(afd_core::error_code::INTERNAL_DB_QUERY.as_str())
    );
    assert_eq!(
        (
            event.field("scanned"),
            event.field("failed"),
            event.field("expired")
        ),
        (Some("2"), Some("1"), Some("1"))
    );
}

pub(super) async fn extra_unissued(fixture: &Decisions<'_>) -> String {
    let source = fixture.case.claimed_source().await;
    let mut input: DecisionReservationInput = fixture
        .reservation(DecisionAssessment::Message, None)
        .into_inner();
    input.source_admission_id = source.to_string();
    let input = garde::Unvalidated::new(input)
        .validate()
        .expect("distinct claimed human source");
    let call = unissued(fixture, &input).await;
    // Input settlement remains separate work. Seed its completed-source state
    // so this financial fixture preserves FIFO ordering.
    let finished = sqlx::query("UPDATE fleet.admissions SET settled_at=clock_timestamp(),parked_at=NULL,updated_at=clock_timestamp() WHERE id=$1::uuid AND input_claim_lease_id=$2::uuid AND input_claim_fence=$3 AND settled_at IS NULL")
        .bind(source.as_str()).bind(fixture.case.held.lease_key.as_str()).bind(fixture.case.held.fence.as_i64())
        .execute(&mut *fixture.case.held.fixtures.database.acquire().await.expect("owned completed input"))
        .await.expect("preceding claimed input finishes before the next source");
    assert_eq!(finished.rows_affected(), 1);
    call
}

pub(super) async fn due_after_scan(fixture: &Decisions<'_>, call: &str) {
    // Advance past the stored millisecond deadline quantum without backdating
    // a new arrival into the already-started scan cycle.
    tokio::time::sleep(Duration::from_millis(2)).await;
    sqlx::query("UPDATE runner.decision_calls SET logical_expires_at=date_trunc('milliseconds',clock_timestamp()) WHERE id=$1::uuid")
        .bind(call).execute(&mut *fixture.case.held.fixtures.database.acquire().await.expect("new elapsed deadline"))
        .await.expect("arrival after the preceding scan");
}
