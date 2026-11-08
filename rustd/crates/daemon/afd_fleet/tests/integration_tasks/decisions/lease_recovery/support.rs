//! Native provider and runner configuration precede ordinary task reissuance.
use super::super::{super::fixture::Case, fixture::Decisions};
use af_core::id::Uuid7;
use afd_db::test_util::{DefaultSeed, PlatformDefault};
use afd_fleet::lease::Acquired;
use afd_vault::{SecretBody, SecretName};
use serde_json::{json, value::RawValue};
use std::borrow::Cow;

pub(in super::super) async fn provider(fixture: &Decisions<'_>) -> PlatformDefault {
    let case = fixture.case;
    case.held
        .fixtures
        .seed_model_rate(crate::seed::PROVIDER, crate::seed::MODEL)
        .await;
    let body = RawValue::from_string(
        json!({"provider":crate::seed::PROVIDER,"api_key":"sk-fixture-not-a-credential"})
            .to_string(),
    )
    .expect("private synthetic provider body");
    fixture
        .vault
        .create(
            &case.scope.workspace_id,
            &SecretName::parse(crate::seed::PROVIDER).expect("provider name"),
            &SecretBody::parse(&body).expect("provider credential"),
            af_core::clock::now(),
        )
        .await
        .expect("encrypted coding-provider key");
    PlatformDefault::hold(
        &case.held.fixtures.database,
        DefaultSeed {
            provider: crate::seed::PROVIDER,
            source_workspace_id: &case.scope.workspace_id,
            model: crate::seed::MODEL,
            context_cap_tokens: 200_000,
            created_at: case.held.fixtures.enrolled_at(),
        },
    )
    .await
}

pub(in super::super) async fn model(
    fixture: &Decisions<'_>,
    selected: Option<&af_core::id::Uuid7>,
) {
    let runners = fixture.case.held.fixtures.runners();
    let row = runners
        .self_record(&fixture.case.held.runner)
        .await
        .expect("native runner assignment");
    let mut policy = row.assignment.decode().expect("stored assignment");
    policy.decision_model_id = selected.map(|id| Cow::Borrowed(id.as_str()));
    runners
        .edit(
            &fixture.case.held.runner,
            afd_runner::RunnerEdit {
                assigned_policy: Some(&policy),
                labels: None,
            },
            af_core::clock::now(),
        )
        .await
        .expect("reviewed native decision model selection");
}

pub(in super::super) async fn expire(case: &Case) {
    chat_source(case).await;
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("elapsed fixture clock");
    sqlx::query("UPDATE runner.fleet_slots SET lease_expires_at=clock_timestamp()-interval '1 second',hold_expires_at=NULL WHERE fleet_id=$1")
        .bind(&case.held.fleet).execute(&mut *connection).await.expect("prior slot elapsed");
    sqlx::query("UPDATE runner.leases SET status='expired' WHERE id=$1")
        .bind(&case.held.lease_key)
        .execute(&mut *connection)
        .await
        .expect("old execution expired");
}

pub(in super::super) async fn claim(case: &Case) -> Acquired {
    let acquired =
        crate::seed::select_fleet(&case.held.leases, &case.held.runner, &case.held.fleet)
            .await
            .expect("native admission reclaim");
    assert_eq!(acquired.event_id, case.held.event_id);
    acquired
}

pub(super) async fn assert_issued(case: &Case, task: &Uuid7, lease: &Uuid7, fence: i64) {
    let row: (Uuid7, i64, i64, i64, bool, bool, i32) = sqlx::query_as("SELECT l.task_id,l.task_generation,l.task_stop_generation,l.fencing_token,l.lease_expires_at<=l.task_execution_deadline_at AND l.lease_expires_at=(SELECT s.lease_expires_at FROM runner.fleet_slots s WHERE s.fleet_id=l.fleet_id),l.task_execution_deadline_at<=t.expires_at AND l.task_execution_deadline_at<=l.task_started_at+interval '600 seconds',t.used_leases FROM runner.leases l JOIN fleet.tasks t ON t.id=l.task_id WHERE l.id=$1")
        .bind(lease).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("issued ownership observer")).await.expect("ordinary lease owns exact task");
    assert_eq!(row, (task.clone(), 1, 0, fence, true, true, 2));
    let rows: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM runner.leases WHERE fleet_id=$1),(SELECT count(*) FROM runner.leases WHERE fleet_id=$1 AND status='active')")
        .bind(&case.scope.fleet_id).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("single execution observer")).await.expect("old and new leases");
    assert_eq!(rows, (2, 1));
}

pub(super) async fn assert_blocked(case: &Case, task: &Uuid7) {
    let row: (String, String, i64, i64, i32, i64) = sqlx::query_as("SELECT status,status_reason,generation,stop_generation,used_leases,(SELECT count(*) FROM runner.leases WHERE fleet_id=$2) FROM fleet.tasks WHERE id=$1")
        .bind(task).bind(&case.scope.fleet_id).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("refusal observer")).await.expect("blocked original task");
    assert_eq!(
        row,
        (
            "blocked".to_owned(),
            "setup_authority_changed".to_owned(),
            2,
            1,
            1,
            1
        )
    );
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fleet.admissions WHERE task_id=$1 AND settled_at IS NULL",
    )
    .bind(task)
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("continuation observer"),
    )
    .await
    .expect("preserved original human source");
    assert_eq!(
        live, 1,
        "original human source is not falsely settled by an authority refusal"
    );
}

async fn chat_source(case: &Case) {
    // The shared direct-issue seed predates EventType and stores the producer
    // spelling as its event kind. Full Plane issuance requires a native Chat.
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("native event fixture");
    let admission =
        sqlx::query("UPDATE fleet.admissions SET event_type=$2 WHERE id=$1 AND event_type='steer'")
            .bind(&case.held.event_id)
            .bind(af_wire::event::EventType::Chat.as_str())
            .execute(&mut *connection)
            .await
            .expect("native human admission kind");
    assert_eq!(admission.rows_affected(), 1);
    let event = sqlx::query("UPDATE fleet.events SET event_type=$3 WHERE fleet_id=$1 AND event_id=$2 AND event_type='steer'")
        .bind(&case.held.fleet).bind(&case.held.event_id).bind(af_wire::event::EventType::Chat.as_str())
        .execute(&mut *connection).await.expect("same native history kind");
    assert_eq!(event.rows_affected(), 1);
}
