//! Busy issuers retain capacity; concurrent recovery releases it once.

use std::time::Duration;

use afd_fleet::task::{DecisionMaintenanceCursor, TaskLifecycle as _};
use afd_wire::task::DecisionAssessment;
use sqlx::Connection as _;
use tokio::time::{sleep, timeout};

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, known, lock_slot},
    maintenance::{budget, due, state, unissued},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_inflight_issuer_keeps_capacity_when_recovery_skips_its_slot() {
    exercise(issuance_wins).await;
}

async fn issuance_wins(case: &Case) {
    let fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let call = unissued(&fixture, &input).await;
    let database = &case.held.fixtures.database;
    sqlx::query("UPDATE runner.decision_calls SET logical_expires_at=clock_timestamp()+interval '1 second' WHERE id=$1::uuid")
        .bind(&call).execute(&mut *database.acquire().await.expect("short fixture deadline"))
        .await.expect("valid future issuance window");
    let mut connection = database.acquire().await.expect("in-flight issuer");
    let mut issuer = connection
        .begin()
        .await
        .expect("native issuance transaction");
    let _pid = lock_slot(&mut issuer, case).await;
    let grant = sqlx::query("UPDATE runner.decision_calls SET issued_at=clock_timestamp(), status=$2 WHERE id=$1::uuid AND issued_at IS NULL AND logical_expires_at>clock_timestamp()")
        .bind(&call).bind("issued").execute(&mut *issuer).await.expect("issuance begins before expiry");
    assert_eq!(grant.rows_affected(), 1);
    sleep(Duration::from_millis(1_050)).await;
    let swept = timeout(
        Duration::from_secs(1),
        fixture
            .tasks
            .maintain_decisions(&mut DecisionMaintenanceCursor::default()),
    )
    .await
    .expect("busy issuance does not stall recovery")
    .expect("complete deferred pass");
    assert_eq!(
        (swept.scanned, swept.expired, swept.deferred, swept.failed),
        (1, 0, 1, 0)
    );
    assert_eq!(
        state(&fixture, &call).await,
        ("reserved".into(), false, false, None)
    );
    issuer.commit().await.expect("delayed issuance commit");
    assert_eq!(
        state(&fixture, &call).await,
        ("issued".into(), true, false, None)
    );
    assert_eq!(fixture.counts().await, (1, 1, 0));
    let (key, replay) = fixture.reserve(&input).await;
    assert!(!key);
    assert_eq!(replay.get("state"), Some(&serde_json::json!("issued")));
    account_late(&fixture, &call).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn competing_expirers_preserve_one_budget_release_and_canonical_replay() {
    exercise(competing).await;
}

async fn competing(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let input = fixture.reservation(
        DecisionAssessment::Goal {
            action_group_id: afd_db::test_util::mint_id(),
        },
        Some(task.clone()),
    );
    let call = unissued(&fixture, &input).await;
    due(&fixture, std::slice::from_ref(&call)).await;
    let mut first_cursor = DecisionMaintenanceCursor::default();
    let mut second_cursor = DecisionMaintenanceCursor::default();
    let (first, second) = tokio::join!(
        fixture.tasks.maintain_decisions(&mut first_cursor),
        fixture.tasks.maintain_decisions(&mut second_cursor),
    );
    let first = first.expect("first complete sweep");
    let second = second.expect("second complete sweep");
    assert_eq!(first.expired + second.expired, 1);
    assert!((1..=2).contains(&(first.scanned + second.scanned)));
    assert_eq!(first.failed + second.failed, 0);
    assert_eq!(
        state(&fixture, &call).await,
        ("not_sent".into(), false, true, Some(0))
    );
    assert_eq!(budget(&fixture, &task.task_id).await, (0, 0));
    assert_eq!(fixture.counts().await, (1, 1, 0));
    let (key, replay) = fixture.reserve(&input).await;
    assert!(!key);
    assert_eq!(replay.get("state"), Some(&serde_json::json!("not_sent")));
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn account_late(fixture: &Decisions<'_>, call: &str) {
    let request: sqlx::types::Json<afd_wire::decision::DecisionRequest> = sqlx::query_scalar(
        "SELECT request FROM runner.decision_payloads WHERE decision_call_id=$1::uuid",
    )
    .bind(call)
    .fetch_one(
        &mut *fixture
            .case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("saved native request"),
    )
    .await
    .expect("retained frozen request");
    let result = fixture.record(call, known(&request.0)).await;
    assert_eq!(result.charged_nanos.as_deref(), Some("3"));
    assert_eq!(
        state(fixture, call).await,
        ("known".into(), true, true, Some(3))
    );
    assert_eq!(fixture.counts().await, (1, 1, 1));
    let swept = fixture
        .tasks
        .maintain_decisions(&mut DecisionMaintenanceCursor::default())
        .await
        .expect("settled rows stay terminal");
    assert_eq!((swept.scanned, swept.expired), (0, 0));
}
