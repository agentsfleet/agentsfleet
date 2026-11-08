//! Routing, accepted task budgets and actual overages keep distinct owners.

use af_core::error_code;
use af_wire::{
    decision::DecisionUsage,
    task::{DecisionAssessment, DecisionCallState, DecisionTaskVersion},
};
use afd_fleet::task::TaskLifecycle as _;
use serde_json::{Value, json};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_new_source_uses_its_own_allowance_while_actual_goal_overage_blocks_related_work() {
    exercise(owners).await;
}

async fn owners(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    source_routing(&fixture, &task).await;
    goal_overage(&fixture, task).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn source_routing(fixture: &Decisions<'_>, task: &DecisionTaskVersion) {
    let source = fixture
        .case
        .source(
            afd_admission::Producer::Steer,
            &afd_events::steer_actor("native_decision_owner"),
        )
        .await;
    let mut input = (*fixture.reservation(DecisionAssessment::Message, Some(task.clone()))).clone();
    input.source_admission_id = source.to_string();
    let input = garde::Unvalidated::new(input)
        .validate()
        .expect("new input while another task runs");
    let (_, body) = fixture.reserve(&input).await;
    let call = body
        .get("call_id")
        .and_then(Value::as_str)
        .expect("source-owned physical call");
    let task_before = fixture
        .tasks
        .read(&fixture.case.scope, &id(&task.task_id))
        .await
        .expect("running task");
    assert_eq!(
        (
            task_before.context.usage.model_nanos,
            task_before.context.usage.reserved_model_nanos,
            task_before.context.usage.decision_calls
        ),
        (0, 0, 0)
    );
    let owner: (Option<String>, String, String) = sqlx::query_as(
        "SELECT task_id::text,source_admission_id::text,context_snapshot #>> '{execution,task_id}' FROM runner.decision_calls WHERE id=$1::uuid")
        .bind(call).fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("native call ownership"))
        .await.expect("retained source attribution");
    assert_eq!(owner, (None, source.to_string(), task.task_id.clone()));
    fixture.reprice().await;
    source_charge(fixture, call, &body).await;
    assert_eq!(
        fixture
            .tasks
            .read(&fixture.case.scope, &id(&task.task_id))
            .await
            .expect("source billing does not move task cursors"),
        task_before
    );
}

async fn source_charge(fixture: &Decisions<'_>, call: &str, body: &Value) {
    let recorded = fixture.record(call, known(&request(body))).await;
    assert_eq!(
        (
            recorded.state,
            recorded.charged_nanos.as_deref(),
            recorded.usage
        ),
        (
            DecisionCallState::Known,
            Some("3"),
            Some(DecisionUsage {
                input_tokens: 1000,
                output_tokens: 100,
                cached_input_tokens: 200,
                cache_write_tokens: 0
            })
        )
    );
}

async fn goal_overage(fixture: &Decisions<'_>, task: DecisionTaskVersion) {
    let input = fixture.reservation(
        DecisionAssessment::Goal {
            action_group_id: afd_db::test_util::mint_id(),
        },
        Some(task.clone()),
    );
    let (_, body) = fixture.reserve(&input).await;
    let call = body
        .get("call_id")
        .and_then(Value::as_str)
        .expect("task-owned physical call");
    let result = fixture.record(call, known(&request(&body))).await;
    assert_eq!(
        (
            result.state,
            result.charged_nanos.as_deref(),
            result.overage_nanos.as_deref(),
            result.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Known,
            Some("40000002"),
            Some("20000002"),
            "0"
        )
    );
    let saved = fixture
        .tasks
        .read(&fixture.case.scope, &id(&task.task_id))
        .await
        .expect("accounted goal budget");
    assert_eq!(
        (
            saved.context.usage.model_nanos,
            saved.context.usage.reserved_model_nanos,
            saved.context.usage.decision_calls
        ),
        (40_000_002, 0, 1)
    );
    further_work_is_blocked(fixture, task).await;
    assert_eq!(
        body.pointer("/dispatch/request/purpose"),
        Some(&json!("goal"))
    );
}

async fn further_work_is_blocked(fixture: &Decisions<'_>, task: DecisionTaskVersion) {
    let next = fixture.reservation(
        DecisionAssessment::Goal {
            action_group_id: afd_db::test_util::mint_id(),
        },
        Some(task),
    );
    let refused = fixture
        .tasks
        .reserve_decision(fixture.case.lease(), &next)
        .await
        .expect_err("actual overage blocks another paid call");
    assert_eq!(refused.code(), error_code::TASK_LIMIT_EXHAUSTED);
    assert_eq!(
        refused.detail(),
        "a decision cost exceeded its reservation; related paid work is blocked"
    );
    assert_eq!(fixture.counts().await, (2, 2, 2));
    let lines: Vec<(String, i64)> = sqlx::query_as("SELECT event_id,charged_nanos FROM billing.usage_lines WHERE work_id=$1::uuid AND seq >= $2 ORDER BY seq")
        .bind(fixture.case.held.lease_key.as_str()).bind(i64::from(af_wire::usage::DECISION_SEQ_BASE))
        .fetch_all(&mut *fixture.case.held.fixtures.database.acquire().await.expect("source and goal charges"))
        .await.expect("two original-work lines");
    assert_eq!(
        lines
            .iter()
            .map(|(_, charged)| *charged)
            .collect::<Vec<_>>(),
        [3, 40_000_002]
    );
    assert_eq!(
        lines.last().expect("goal line").0,
        fixture.case.held.event_id
    );
    assert_ne!(
        lines.first().expect("new source line").0,
        fixture.case.held.event_id
    );
}
