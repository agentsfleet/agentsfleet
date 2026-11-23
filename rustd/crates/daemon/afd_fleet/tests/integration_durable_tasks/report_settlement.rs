//! Semantic refusal preserves financial settlement through the ordinary report path.

use super::fixture::{Case, exercise, id};
use af_core::id::Uuid7;
use af_wire::{
    durable_task::{
        DecisionDurableTaskVersion, DurableTaskDisposition, DurableTaskReport, DurableTaskState,
    },
    report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry},
};
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTaskReportOwner as _};
use std::{borrow::Cow, sync::Arc};

pub(super) fn request(case: &Case, task: Option<DurableTaskReport>) -> ReportRequest<'_> {
    ReportRequest {
        folded_through_sequence: None,
        setup: None,
        durable_task: task,
        run_id: Cow::Borrowed(case.held.run_key.as_str()),
        turn_id: Cow::Borrowed(case.held.turn_id.as_str()),
        fencing_token: case.run().fencing_token,
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed("The runner claims the requested work is complete."),
        tokens: 0,
        requests: af_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: 1000,
            segments: None,
        },
        checkpoint: ReportCheckpoint {
            last_turn_id: Cow::Borrowed(case.held.turn_id.as_str()),
            last_response: Cow::Borrowed("ordinary session cursor"),
        },
        tool_calls: None,
        held_until: None,
        memory: Vec::new(),
    }
}

async fn task(case: &Case, disposition: DurableTaskDisposition) -> DurableTaskReport {
    let bindings = case
        .tasks
        .bind(case.run(), &case.plan("report-goal", 1))
        .await
        .expect("accepted API-only task");
    let durable_task_id = bindings
        .items
        .first()
        .expect("one task")
        .durable_task_id
        .as_ref()
        .expect("task owner")
        .to_string();
    DurableTaskReport {
        durable_task: DecisionDurableTaskVersion {
            durable_task_id,
            generation: 1,
        },
        disposition,
        decision_call_id: None,
        checkpoint_id: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
    }
}

async fn settle(case: &Case, request: &ReportRequest<'_>, durable_task_id: &str) {
    let durable_task_id = id(durable_task_id);
    let mut plane = case.held.fixtures.plane();
    plane.durable_task_reports = Some(Arc::new(case.tasks.clone()));
    let now = af_core::clock::after(case.held.now, crate::report_seed::SLICE);
    plane
        .report_with(
            &case.held.runner,
            request,
            now,
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("ordinary financial report");
    let snapshot = case
        .tasks
        .read(&case.scope, &durable_task_id)
        .await
        .expect("settled task");
    assert_eq!(snapshot.context.state, DurableTaskState::Blocked);
    assert_eq!(snapshot.context.generation, 2);
    assert_eq!(snapshot.context.stop_generation, 1);
    assert!(snapshot.context.usage.running_nanos > 0);
    assert_accounted(case, &durable_task_id).await;
    plane
        .report_with(
            &case.held.runner,
            request,
            now,
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("terminal report retry");
    assert_eq!(
        case.tasks
            .read(&case.scope, &durable_task_id)
            .await
            .expect("stable replay"),
        snapshot
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn native_blocked_report_settles_once_without_completion_claims() {
    exercise(async |case| {
        let task = task(case, DurableTaskDisposition::Blocked).await;
        let durable_task_id = task.durable_task.durable_task_id.clone();
        let report = request(case, Some(task));
        assert!(
            case.tasks
                .prepare_report(case.run(), &report)
                .await
                .expect("native blocked evidence")
                .is_some()
        );
        settle(case, &report, &durable_task_id).await;
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn missing_semantic_report_preserves_charges_and_blocks_unverified_completion() {
    exercise(async |case| {
        let task = task(case, DurableTaskDisposition::Complete).await;
        let report = request(case, None);
        assert!(
            case.tasks
                .prepare_report(case.run(), &report)
                .await
                .expect("ordinary report")
                .is_none()
        );
        settle(case, &report, &task.durable_task.durable_task_id).await;
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn continuation_without_recovery_checkpoint_settles_but_never_wakes() {
    exercise(async |case| {
        let task = task(case, DurableTaskDisposition::Continue).await;
        let durable_task_id = task.durable_task.durable_task_id.clone();
        let report = request(case, Some(task));
        assert!(
            case.tasks
                .prepare_report(case.run(), &report)
                .await
                .expect("continuation preparation")
                .is_some()
        );
        settle(case, &report, &durable_task_id).await;
        assert_eq!(case.counts().await, (1, 1, 1));
    })
    .await;
}

async fn assert_accounted(case: &Case, durable_task_id: &Uuid7) {
    let stored: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM billing.usage_lines WHERE work_id=$2) FROM fleet.durable_tasks t WHERE t.id=$1",
    ).bind(durable_task_id).bind(&case.held.run_key)
        .fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("financial observer"))
        .await.expect("task and measured charge committed together");
    assert_eq!(stored, 1);
    assert!(case.held.fixtures.turn_settled(&case.held.turn_id).await);
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn malformed_stale_and_unpaid_semantic_reports_never_prepare_a_permit() {
    exercise(async |case| {
        let input = task(case, DurableTaskDisposition::Blocked).await;
        let durable_task_id = id(&input.durable_task.durable_task_id);
        let before = case
            .tasks
            .read(&case.scope, &durable_task_id)
            .await
            .expect("running objective");
        for refused in invalid_reports(&input) {
            let report = request(case, Some(refused));
            assert!(
                case.tasks
                    .prepare_report(case.run(), &report)
                    .await
                    .expect("semantic refusal leaves transport available")
                    .is_none()
            );
            assert_eq!(
                case.tasks
                    .read(&case.scope, &durable_task_id)
                    .await
                    .expect("unchanged task"),
                before
            );
        }
        assert!(!case.held.fixtures.turn_settled(&case.held.turn_id).await);
        assert_eq!(case.counts().await, (1, 1, 1));
    })
    .await;
}

fn invalid_reports(original: &DurableTaskReport) -> Vec<DurableTaskReport> {
    let mut malformed = original.clone();
    "not-a-task-id".clone_into(&mut malformed.durable_task.durable_task_id);
    let mut stale = original.clone();
    stale.durable_task.generation = 2;
    let mut unpaid_complete = original.clone();
    unpaid_complete.disposition = DurableTaskDisposition::Complete;
    let mut unpaid_wait = original.clone();
    unpaid_wait.disposition = DurableTaskDisposition::Waiting;
    let mut missing_call = unpaid_complete.clone();
    missing_call.decision_call_id = Some(afd_db::test_util::mint_id().into());
    vec![malformed, stale, unpaid_complete, unpaid_wait, missing_call]
}
