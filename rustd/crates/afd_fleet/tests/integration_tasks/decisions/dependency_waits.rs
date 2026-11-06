//! Persisted dependency recovery fixtures, not evidence of report-time wait arming.

use afd_db::test_util::mint_id;
use afd_fleet::task::TaskLifecycle as _;
use serde_json::{Value, json};
use sqlx::types::Json;

use super::{
    super::fixture::{Case, exercise},
    fixture::Decisions,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn due_wait_without_repository_authority_blocks_without_agent_or_fallback_charge() {
    exercise(authority).await;
}

async fn authority(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    seed_wait(case, &task.task_id, false).await;
    let before = case.counts().await;
    let result = fixture
        .tasks
        .maintain_waits()
        .await
        .expect("revoked dependency maintenance");
    assert_eq!((result.checked, result.woken, result.deferred), (0, 0, 0));
    let row = task_snapshot(case, &task.task_id).await;
    assert_eq!(row["status"], "blocked");
    assert_eq!(row["status_reason"], "dependency_access_unavailable");
    assert_eq!(row["generation"], 2);
    assert_eq!(row["stop_generation"], 1);
    assert_eq!(row["used_fallback_reads"], 0);
    assert!(row["terminal_at"].is_null());
    assert!(!wait_snapshot(case, &task.task_id).await["cancelled_at"].is_null());
    fixture
        .tasks
        .maintain_waits()
        .await
        .expect("repeat blocked maintenance");
    assert_eq!(task_snapshot(case, &task.task_id).await, row);
    assert_eq!(case.counts().await, before);
    fixture.model.finish().await.expect("fixture cleanup");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn expired_wait_exhausts_once_even_during_provider_cooldown() {
    exercise(expiration).await;
}

async fn expiration(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    seed_wait(case, &task.task_id, true).await;
    sqlx::query("UPDATE fleet.tasks SET expires_at=now()-interval '1 second' WHERE id=$1::uuid")
        .bind(&task.task_id)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("deadline fixture"),
        )
        .await
        .expect("elapsed wall deadline");
    let before = case.counts().await;
    fixture
        .tasks
        .maintain_waits()
        .await
        .expect("native wall enforcement");
    let row = task_snapshot(case, &task.task_id).await;
    assert_eq!(row["status"], "exhausted");
    assert_eq!(row["status_reason"], "wall_deadline_reached");
    assert_eq!(row["generation"], 2);
    assert!(!row["terminal_at"].is_null());
    assert!(!wait_snapshot(case, &task.task_id).await["cancelled_at"].is_null());
    fixture
        .tasks
        .maintain_waits()
        .await
        .expect("repeat terminal maintenance");
    assert_eq!(task_snapshot(case, &task.task_id).await, row);
    assert_eq!(case.counts().await, before);
    fixture.model.finish().await.expect("fixture cleanup");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn changed_head_cancels_old_wait_without_reading_or_stopping_current_task() {
    exercise(changed_head).await;
}

async fn changed_head(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    seed_wait(case, &task.task_id, false).await;
    sqlx::query("UPDATE fleet.tasks SET target=jsonb_set(target,'{head}',to_jsonb($2::text)) WHERE id=$1::uuid")
        .bind(&task.task_id).bind("2".repeat(40))
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("changed native head fixture"))
        .await.expect("preexisting newer head");
    let before = task_snapshot(case, &task.task_id).await;
    let counts = case.counts().await;
    let result = fixture
        .tasks
        .maintain_waits()
        .await
        .expect("superseded wait maintenance");
    assert_eq!((result.checked, result.woken, result.deferred), (0, 0, 0));
    assert_eq!(task_snapshot(case, &task.task_id).await, before);
    let wait = wait_snapshot(case, &task.task_id).await;
    assert!(!wait["cancelled_at"].is_null());
    assert!(wait["claim_id"].is_null());
    assert_eq!(case.counts().await, counts);
    fixture.model.finish().await.expect("fixture cleanup");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn exhausted_model_budget_cancels_wait_before_authority_or_network_read() {
    exercise(cumulative_limit).await;
}

async fn cumulative_limit(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    seed_wait(case, &task.task_id, false).await;
    sqlx::query("UPDATE fleet.tasks SET used_model_nanos=max_model_nanos WHERE id=$1::uuid")
        .bind(&task.task_id)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("spent budget fixture"),
        )
        .await
        .expect("preexisting spent model budget");
    let before = case.counts().await;
    let result = fixture
        .tasks
        .maintain_waits()
        .await
        .expect("bounded cumulative enforcement");
    assert_eq!((result.checked, result.woken, result.deferred), (0, 0, 0));
    let row = task_snapshot(case, &task.task_id).await;
    assert_eq!(row["status"], "exhausted");
    assert_eq!(row["status_reason"], "cumulative_limit_reached");
    assert_eq!(row["used_fallback_reads"], 0);
    assert!(!row["terminal_at"].is_null());
    assert!(!wait_snapshot(case, &task.task_id).await["cancelled_at"].is_null());
    assert_eq!(case.counts().await, before);
    fixture.model.finish().await.expect("fixture cleanup");
}

pub(super) async fn seed_wait(case: &Case, task: &str, cooling: bool) {
    let checks = json!([{"workflow_id":42,"run_id":100,"run_number":10,"run_attempt":2,"status":"in_progress","conclusion":null}]);
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("persisted wait fixture");
    sqlx::query("UPDATE fleet.tasks SET status='waiting',wait_sequence=1 WHERE id=$1::uuid")
        .bind(task)
        .execute(&mut *connection)
        .await
        .expect("preexisting waiting task");
    sqlx::query("INSERT INTO fleet.task_waits (id,task_id,fleet_id,workspace_id,owner_lease_id,task_generation,stop_generation,generation,installation_id,repository_id,pull_request_number,expected_head,checks,baseline_digest,due_at,expires_at,cooldown_expires_at,fallback_count,signal_sequence,observed_signal_sequence,created_at,updated_at) SELECT $1::uuid,id,fleet_id,workspace_id,$3::uuid,generation,stop_generation,1,1,101,101,$4,$5,$6,now()-interval '1 second',expires_at,CASE WHEN $7 THEN now()+interval '1 hour' ELSE now()-interval '1 second' END,0,0,0,now(),now() FROM fleet.tasks WHERE id=$2::uuid")
        .bind(mint_id()).bind(task).bind(case.held.lease_key.as_str()).bind("0".repeat(40))
        .bind(Json(checks)).bind("a".repeat(64)).bind(cooling)
        .execute(&mut *connection).await.expect("persisted dependency, no fabricated observation");
}

pub(super) async fn wait_snapshot(case: &Case, task: &str) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(w) FROM fleet.task_waits w WHERE task_id=$1::uuid")
        .bind(task)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("wait observer"),
        )
        .await
        .expect("persisted wait")
}

async fn task_snapshot(case: &Case, task: &str) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(t) FROM fleet.tasks t WHERE id=$1::uuid")
        .bind(task)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("task observer"),
        )
        .await
        .expect("persisted task")
}
