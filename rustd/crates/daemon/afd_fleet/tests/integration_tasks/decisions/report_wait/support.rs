//! Real decision receipts, external GitHub observations and committed ledger assertions.
use super::super::super::fixture::Case;
use super::super::{
    fixture::{Decisions, request},
    github_server::Server,
    interpretation::choose,
    reply::known_result,
};
use af_core::id::Uuid7;
use af_wire::{
    decision::DecisionOutcome,
    task::{DecisionReceipt, DecisionTaskVersion, TaskDisposition, TaskReport, TaskState},
};
use afd_fleet::task::TaskSnapshot;
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub(super) enum Observation {
    Success,
    Failure,
    Pending,
    Missing,
    ChangedHead,
    Closed,
}

pub(super) async fn decision(
    fixture: &Decisions<'_>,
    task: DecisionTaskVersion,
    checkpoint: &str,
) -> TaskReport {
    let (_, body) = fixture.reserve(&fixture.goal_reservation(&task)).await;
    let request = request(&body);
    let mut result = known_result(&request);
    let answers = match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("paid goal answers");
    choose(answers, "next_action", "WAIT");
    fixture
        .record(&request.call_id, DecisionReceipt::Provider { result })
        .await;
    TaskReport {
        task,
        disposition: TaskDisposition::Waiting,
        decision_call_id: Some(request.call_id),
        checkpoint_id: Some(checkpoint.to_owned()),
        covered_input_sequence: 0,
        effect_watermark: 0,
    }
}

pub(super) fn observe(server: &Server, observation: Observation) {
    let head = if matches!(observation, Observation::ChangedHead) {
        "f"
    } else {
        "0"
    }
    .repeat(40);
    let state = if matches!(observation, Observation::Closed) {
        "closed"
    } else {
        "open"
    };
    let repository = json!({"id":101,"full_name":"example/linkwarden"});
    server.push(
        json!({"fixture_request":{"method":"GET","path":"/repos/example/linkwarden/pulls/101"},
        "response":{"number":101,"state":state,"merged":false,
        "head":{"sha":head,"ref":"repair","repo":repository},
        "base":{"sha":"0".repeat(40),"ref":"main","repo":repository}}}),
    );
    if matches!(observation, Observation::ChangedHead | Observation::Closed) {
        return;
    }
    let (status, conclusion) = match observation {
        Observation::Success => ("completed", Some("success")),
        Observation::Failure => ("completed", Some("failure")),
        _ => ("in_progress", None),
    };
    let runs = if matches!(observation, Observation::Missing) {
        vec![]
    } else {
        vec![
            json!({"id":100,"workflow_id":42,"run_attempt":1,"run_number":1,
            "head_sha":head,"status":status,"conclusion":conclusion}),
        ]
    };
    server.push(json!({"fixture_request":{"method":"GET", "path":format!("/repos/example/linkwarden/actions/runs?head_sha={head}&per_page=100")},
        "response":{"total_count":runs.len(),"workflow_runs":runs}}));
}

pub(super) fn assert_state(snapshot: &TaskSnapshot, state: TaskState, checkpoint: &str) {
    assert_eq!(snapshot.context.state, state);
    assert_eq!(snapshot.context.generation, 2);
    assert_eq!(snapshot.context.stop_generation, 1);
    assert_eq!(snapshot.context.checkpoint_id.as_deref(), Some(checkpoint));
    assert_eq!(snapshot.context.usage.model_nanos, 3);
    assert_eq!(snapshot.context.usage.reserved_model_nanos, 0);
    assert!(snapshot.context.usage.running_nanos > 0);
}

pub(super) async fn checkpoint_row(case: &Case, checkpoint: &Uuid7) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(c) FROM fleet.task_checkpoints c WHERE id=$1")
        .bind(checkpoint)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("checkpoint observer"),
        )
        .await
        .expect("committed checkpoint retained")
}

pub(super) async fn counts(case: &Case, task: &Uuid7) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM fleet.admissions WHERE task_id=$1 AND producer='task_continuation' AND settled_at IS NULL), (SELECT COALESCE(max(task_generation),0) FROM fleet.admissions WHERE task_id=$1 AND producer='task_continuation' AND settled_at IS NULL), (SELECT count(*) FROM fleet.task_waits WHERE task_id=$1 AND cancelled_at IS NULL), (SELECT count(*) FROM billing.usage_lines WHERE work_id=$2), (SELECT count(*) FROM fleet.admissions WHERE fleet_id=$3)")
        .bind(task).bind(&case.held.lease_key).bind(&case.held.fleet)
        .fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("ledger observer"))
        .await.expect("native continuation, waits, accounting and admissions")
}

pub(super) fn assert_counts(counts: (i64, i64, i64, i64, i64), state: TaskState) {
    let expected = match state {
        TaskState::Ready => Some((1, 2, 0, 2, 2)),
        TaskState::Waiting => Some((0, 0, 1, 2, 1)),
        TaskState::Blocked => Some((0, 0, 0, 2, 1)),
        _ => None,
    }
    .expect("scenario uses Ready, Waiting or Blocked");
    assert_eq!(
        counts, expected,
        "one native continuation only when work is ready, one charged goal and runtime line"
    );
}
