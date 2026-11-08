//! API-only completion needs paid current goal evidence and settles exactly once.

use super::super::{
    fixture::{Case, exercise, id},
    report_settlement,
};
use super::{
    fixture::{Decisions, request},
    interpretation::{choose, proposed},
    reply::known_result,
};
use af_wire::{
    decision::DecisionOutcome,
    task::{
        DecisionReceipt, DecisionTaskVersion, TaskDisposition, TaskInputAction, TaskReport,
        TaskState,
    },
};
use afd_fleet::task::{TaskLifecycle as _, TaskSnapshot};
use std::sync::Arc;

async fn accepted(fixture: &Decisions<'_>) -> DecisionTaskVersion {
    let input = garde::Unvalidated::new(proposed(fixture, TaskInputAction::NewWork).await)
        .validate()
        .expect("complete paid interpretation");
    let result = fixture
        .tasks
        .interpret(fixture.case.lease(), &input)
        .await
        .expect("native API-only goal");
    let task = result
        .current_task
        .expect("first new goal owns current lease");
    assert_eq!(task.objective, "Repair");
    assert_eq!(task.questions.len(), 2);
    DecisionTaskVersion {
        task_id: task.task_id.to_string(),
        generation: task.generation,
    }
}

async fn completion(fixture: &Decisions<'_>, task: DecisionTaskVersion) -> TaskReport {
    let (_, body) = fixture.reserve(&fixture.goal_reservation(&task)).await;
    let request = request(&body);
    let mut result = known_result(&request);
    let answers = match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("complete configured goal answer");
    choose(answers, "next_action", "DONE");
    fixture
        .record(&request.call_id, DecisionReceipt::Provider { result })
        .await;
    TaskReport {
        task,
        disposition: TaskDisposition::Complete,
        decision_call_id: Some(request.call_id),
        checkpoint_id: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_current_api_only_completion_commits_done_with_both_decision_charges() {
    exercise(done).await;
}

async fn done(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = accepted(&fixture).await;
    super::configured::support::set(case, false).await;
    let task_id = task.task_id.clone();
    let semantic = completion(&fixture, task).await;
    let report = report_settlement::request(case, Some(semantic));
    let mut plane = case.held.fixtures.plane();
    plane.task_reports = Some(Arc::new(fixture.tasks.clone()));
    let now = af_core::clock::after(case.held.now, crate::report_seed::SLICE);
    plane
        .report(&case.held.runner, &report, now)
        .await
        .expect("verified complete report");
    let snapshot = assert_done(&fixture, &task_id).await;
    plane
        .report(&case.held.runner, &report, now)
        .await
        .expect("completed report replay");
    assert_eq!(
        fixture
            .tasks
            .read(&case.scope, &id(&task_id))
            .await
            .expect("same terminal task"),
        snapshot
    );
    assert_eq!(fixture.counts().await, (2, 2, 2));
    fixture.model.finish().await.expect("model cleanup");
}

async fn assert_done(fixture: &Decisions<'_>, task_id: &str) -> TaskSnapshot {
    let case = fixture.case;
    let snapshot = fixture
        .tasks
        .read(&case.scope, &id(task_id))
        .await
        .expect("completed objective");
    assert_eq!(snapshot.context.state, TaskState::Done);
    assert_eq!(snapshot.context.generation, 2);
    assert_eq!(snapshot.context.usage.model_nanos, 3);
    assert!(snapshot.context.usage.running_nanos > 0);
    assert_eq!(fixture.counts().await, (2, 2, 2));
    snapshot
}
