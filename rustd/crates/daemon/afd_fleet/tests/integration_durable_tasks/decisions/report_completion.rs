//! API-only completion needs paid current goal evidence and settles exactly once.

use super::super::{
    fixture::{Case, exercise, id},
    report_settlement,
};
use super::{
    fixture::{Decisions, request},
    reply::known_result,
    steps::{choose, durable_tasks_enabled},
};
use af_wire::{
    decision::DecisionOutcome,
    durable_task::{
        DecisionDurableTaskVersion, DecisionReceipt, DurableTaskDisposition, DurableTaskReport,
        DurableTaskState,
    },
};
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTaskSnapshot};
use std::sync::Arc;

async fn accepted(fixture: &Decisions<'_>) -> DecisionDurableTaskVersion {
    fixture.bind_api_only_goal().await
}

async fn completion(
    fixture: &Decisions<'_>,
    task: DecisionDurableTaskVersion,
) -> DurableTaskReport {
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
    DurableTaskReport {
        durable_task: task,
        disposition: DurableTaskDisposition::Complete,
        decision_call_id: Some(request.call_id),
        checkpoint_id: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_current_api_only_completion_commits_done_with_its_one_goal_charge() {
    exercise(done).await;
}

async fn done(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = accepted(&fixture).await;
    durable_tasks_enabled(case, false).await;
    let durable_task_id = task.durable_task_id.clone();
    let semantic = completion(&fixture, task).await;
    let report = report_settlement::request(case, Some(semantic));
    let mut plane = case.held.fixtures.plane();
    plane.durable_task_reports = Some(Arc::new(fixture.tasks.clone()));
    let now = af_core::clock::after(case.held.now, crate::report_seed::SLICE);
    plane
        .report_with(
            &case.held.runner,
            &report,
            now,
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("verified complete report");
    let snapshot = assert_done(&fixture, &durable_task_id).await;
    plane
        .report_with(
            &case.held.runner,
            &report,
            now,
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("completed report replay");
    assert_eq!(
        fixture
            .tasks
            .read(&case.scope, &id(&durable_task_id))
            .await
            .expect("same terminal task"),
        snapshot
    );
    assert_eq!(fixture.counts().await, (1, 1, 1));
    fixture.model.finish().await.expect("model cleanup");
}

async fn assert_done(fixture: &Decisions<'_>, durable_task_id: &str) -> DurableTaskSnapshot {
    let case = fixture.case;
    let snapshot = fixture
        .tasks
        .read(&case.scope, &id(durable_task_id))
        .await
        .expect("completed objective");
    assert_eq!(snapshot.context.state, DurableTaskState::Done);
    assert_eq!(snapshot.context.generation, 2);
    assert_eq!(snapshot.context.usage.model_nanos, 3);
    assert!(snapshot.context.usage.running_nanos > 0);
    // The goal check is the run's one decision call: starting the task by
    // tool made none.
    assert_eq!(fixture.counts().await, (1, 1, 1));
    snapshot
}
