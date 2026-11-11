//! Semantic refusal preserves financial settlement through the ordinary report path.

use super::fixture::{Case, exercise, id};
use af_core::id::Uuid7;
use af_wire::{
    report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry},
    task::{DecisionTaskVersion, TaskDisposition, TaskReport, TaskState},
};
use afd_fleet::task::{TaskLifecycle as _, TaskReportOwner as _};
use std::{borrow::Cow, sync::Arc};

pub(super) fn request(case: &Case, task: Option<TaskReport>) -> ReportRequest<'_> {
    ReportRequest {
        folded_through_sequence: None,
        task,
        lease_id: Cow::Borrowed(case.held.lease_key.as_str()),
        event_id: Cow::Borrowed(case.held.event_id.as_str()),
        fencing_token: case.lease().fencing_token,
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
            last_event_id: Cow::Borrowed(case.held.event_id.as_str()),
            last_response: Cow::Borrowed("ordinary session cursor"),
        },
        tool_calls: None,
        held_until: None,
        memory: Vec::new(),
    }
}

async fn task(case: &Case, disposition: TaskDisposition) -> TaskReport {
    let bindings = case
        .tasks
        .bind(case.lease(), &case.plan("report-goal", 1))
        .await
        .expect("accepted API-only task");
    let task_id = bindings
        .items
        .first()
        .expect("one task")
        .task_id
        .as_ref()
        .expect("task owner")
        .to_string();
    TaskReport {
        task: DecisionTaskVersion {
            task_id,
            generation: 1,
        },
        disposition,
        decision_call_id: None,
        checkpoint_id: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
    }
}

async fn settle(case: &Case, request: &ReportRequest<'_>, task_id: &str, reason: &str) {
    let task_id = id(task_id);
    let mut plane = case.held.fixtures.plane();
    plane.task_reports = Some(Arc::new(case.tasks.clone()));
    let now = af_core::clock::after(case.held.now, crate::report_seed::SLICE);
    plane
        .report(
            &case.held.runner,
            request,
            now,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("ordinary financial report");
    let snapshot = case
        .tasks
        .read(&case.scope, &task_id)
        .await
        .expect("settled task");
    assert_eq!(snapshot.context.state, TaskState::Blocked);
    assert_eq!(snapshot.context.generation, 2);
    assert_eq!(snapshot.context.stop_generation, 1);
    assert!(snapshot.context.usage.running_nanos > 0);
    assert_accounted(case, &task_id, reason).await;
    plane
        .report(
            &case.held.runner,
            request,
            now,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("terminal report retry");
    assert_eq!(
        case.tasks
            .read(&case.scope, &task_id)
            .await
            .expect("stable replay"),
        snapshot
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn native_blocked_report_settles_once_without_completion_claims() {
    exercise(async |case| {
        let task = task(case, TaskDisposition::Blocked).await;
        let task_id = task.task.task_id.clone();
        let report = request(case, Some(task));
        assert!(
            case.tasks
                .prepare_report(case.lease(), &report)
                .await
                .expect("native blocked evidence")
                .is_some()
        );
        settle(case, &report, &task_id, "prerequisite_unavailable").await;
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn missing_semantic_report_preserves_charges_and_blocks_unverified_completion() {
    exercise(async |case| {
        let task = task(case, TaskDisposition::Complete).await;
        let report = request(case, None);
        assert!(
            case.tasks
                .prepare_report(case.lease(), &report)
                .await
                .expect("ordinary report")
                .is_none()
        );
        settle(case, &report, &task.task.task_id, "completion_unverified").await;
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn continuation_without_recovery_checkpoint_settles_but_never_wakes() {
    exercise(async |case| {
        let task = task(case, TaskDisposition::Continue).await;
        let task_id = task.task.task_id.clone();
        let report = request(case, Some(task));
        assert!(
            case.tasks
                .prepare_report(case.lease(), &report)
                .await
                .expect("continuation preparation")
                .is_some()
        );
        settle(case, &report, &task_id, "recovery_or_completion_unverified").await;
        assert_eq!(case.counts().await, (1, 1, 1));
    })
    .await;
}

async fn assert_accounted(case: &Case, task_id: &Uuid7, reason: &str) {
    let stored: (String, i64) = sqlx::query_as(
        "SELECT t.status_reason,(SELECT count(*) FROM billing.usage_lines WHERE work_id=$2) FROM fleet.tasks t WHERE t.id=$1",
    ).bind(task_id).bind(&case.held.lease_key)
        .fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("financial observer"))
        .await.expect("task and measured charge committed together");
    assert_eq!(stored, (reason.to_owned(), 1));
    assert!(
        case.held
            .fixtures
            .admission_settled(&case.held.event_id)
            .await
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn malformed_stale_and_unpaid_semantic_reports_never_prepare_a_permit() {
    exercise(async |case| {
        let input = task(case, TaskDisposition::Blocked).await;
        let task_id = id(&input.task.task_id);
        let before = case
            .tasks
            .read(&case.scope, &task_id)
            .await
            .expect("running objective");
        for refused in invalid_reports(&input) {
            let report = request(case, Some(refused));
            assert!(
                case.tasks
                    .prepare_report(case.lease(), &report)
                    .await
                    .expect("semantic refusal leaves transport available")
                    .is_none()
            );
            assert_eq!(
                case.tasks
                    .read(&case.scope, &task_id)
                    .await
                    .expect("unchanged task"),
                before
            );
        }
        assert!(
            !case
                .held
                .fixtures
                .admission_settled(&case.held.event_id)
                .await
        );
        assert_eq!(case.counts().await, (1, 1, 1));
    })
    .await;
}

fn invalid_reports(original: &TaskReport) -> Vec<TaskReport> {
    let mut malformed = original.clone();
    "not-a-task-id".clone_into(&mut malformed.task.task_id);
    let mut stale = original.clone();
    stale.task.generation = 2;
    let mut unpaid_complete = original.clone();
    unpaid_complete.disposition = TaskDisposition::Complete;
    let mut unpaid_wait = original.clone();
    unpaid_wait.disposition = TaskDisposition::Waiting;
    let mut missing_call = unpaid_complete.clone();
    missing_call.decision_call_id = Some(afd_db::test_util::mint_id().into());
    vec![malformed, stale, unpaid_complete, unpaid_wait, missing_call]
}
