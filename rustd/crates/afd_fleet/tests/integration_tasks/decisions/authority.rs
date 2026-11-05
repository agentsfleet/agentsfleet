//! Refused associations preserve work; a paused owner's late usage still pays.

use afd_core::error_code;
use afd_fleet::task::{TaskControl, TaskLifecycle as _, TaskSnapshot};
use afd_wire::task::{DecisionCallState, DecisionRecordDisposition, DecisionTaskVersion};
use serde_json::Value;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn refused_goal_owners_preserve_state_and_pause_keeps_late_original_usage() {
    exercise(ownership).await;
}

async fn ownership(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let before = fixture.goal_snapshot(&task).await;
    let balance = fixture.wallet().await;
    let sibling = case.sibling().await;
    let foreign = sibling
        .tasks
        .bind(sibling.lease(), &sibling.plan("other-owner", 1))
        .await
        .expect("real foreign goal");
    let other = foreign
        .items
        .first()
        .expect("foreign binding")
        .task_id
        .as_ref()
        .expect("accepted goal owns a task")
        .to_string();
    refused(&fixture, &task, &other, &before).await;
    sibling.held.fixtures.cleanup().await;
    late(&fixture, &task, balance).await;
    fixture.model.finish().await;
}

async fn late(fixture: &Decisions<'_>, task: &DecisionTaskVersion, balance: i64) {
    let case = fixture.case;
    let input = fixture.goal_reservation(task);
    let (_, body) = fixture.reserve(&input).await;
    let request = request(&body);
    let running = fixture.goal_snapshot(task).await;
    let paused = fixture
        .tasks
        .control(
            &case.scope,
            &id(&task.task_id),
            &running.etag(),
            running.context.generation,
            TaskControl::Pause,
        )
        .await
        .expect("native pause");
    let receipt = known(&request);
    let written = fixture.record(&request.call_id, receipt.clone()).await;
    assert_eq!(
        (
            written.state,
            written.disposition,
            written.charged_nanos.as_deref(),
            written.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Known,
            DecisionRecordDisposition::Stale,
            Some("3"),
            "0"
        )
    );
    let mut expected = paused;
    expected.context.usage.model_nanos = 3;
    expected.context.usage.reserved_model_nanos = 0;
    expected.resource_version += 1;
    assert_eq!(fixture.goal_snapshot(task).await, expected);
    assert!(fixture.record(&request.call_id, receipt).await.replayed);
    assert_eq!(fixture.goal_snapshot(task).await, expected);
    assert_eq!(fixture.wallet().await, balance - 3);
    assert_eq!(fixture.counts().await, (1, 1, 1));
    let generation: (i64, String) = sqlx::query_as("SELECT task_generation,context_snapshot #>> '{execution,generation}' FROM runner.decision_calls WHERE id=$1::uuid")
        .bind(&request.call_id).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("original generation observer")).await.expect("native generation binding");
    assert_eq!(generation, (1, "1".to_owned()));
}

async fn refused(
    fixture: &Decisions<'_>,
    task: &DecisionTaskVersion,
    foreign: &str,
    before: &TaskSnapshot,
) {
    let source_before = admission(fixture, &fixture.case.held.event_id).await;
    for (version, expected) in [
        (None, error_code::TASK_INVALID_TARGET),
        (
            Some(DecisionTaskVersion {
                task_id: foreign.to_owned(),
                generation: 1,
            }),
            error_code::TASK_STALE_STATE,
        ),
        (
            Some(DecisionTaskVersion {
                task_id: task.task_id.clone(),
                generation: 2,
            }),
            error_code::TASK_STALE_STATE,
        ),
    ] {
        let mut input = (*fixture.goal_reservation(task)).clone();
        input.task = version;
        let input = garde::Unvalidated::new(input)
            .validate()
            .expect("well-formed refused association");
        let fault = fixture
            .tasks
            .reserve_decision(fixture.case.lease(), &input)
            .await
            .expect_err("current owner and generation are mandatory");
        assert_eq!(fault.code(), expected);
        assert_eq!(fixture.goal_snapshot(task).await, *before);
        assert_eq!(
            admission(fixture, &fixture.case.held.event_id).await,
            source_before
        );
        assert_eq!(fixture.counts().await, (0, 0, 0));
    }
    wrong_source(fixture, task, before).await;
}

async fn wrong_source(fixture: &Decisions<'_>, task: &DecisionTaskVersion, before: &TaskSnapshot) {
    let source = fixture
        .case
        .source(
            afd_admission::Producer::Steer,
            &afd_events::steer_actor("goal-owner"),
        )
        .await;
    let source_before = admission(fixture, source.as_str()).await;
    let mut input = (*fixture.goal_reservation(task)).clone();
    input.source_admission_id = source.to_string();
    let input = garde::Unvalidated::new(input)
        .validate()
        .expect("distinct admitted source");
    let fault = fixture
        .tasks
        .reserve_decision(fixture.case.lease(), &input)
        .await
        .expect_err("another admission cannot own the goal assessment");
    assert_eq!(fault.code(), error_code::TASK_INVALID_TARGET);
    assert_eq!(admission(fixture, source.as_str()).await, source_before);
    assert_eq!(fixture.goal_snapshot(task).await, *before);
    assert_eq!(fixture.counts().await, (0, 0, 0));
}

async fn admission(fixture: &Decisions<'_>, source: &str) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(a) FROM fleet.admissions a WHERE id=$1::uuid")
        .bind(source)
        .fetch_one(
            &mut *fixture
                .case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("source observer"),
        )
        .await
        .expect("complete original admission")
}
