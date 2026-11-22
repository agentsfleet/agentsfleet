//! A report's task evidence counts only for the task, and the generation, the
//! run is executing.

use af_core::{error_code, test_util::trace::Capture};
use af_wire::task::{DecisionTaskVersion, TaskDisposition, TaskReport};
use afd_fleet::task::{TaskControl, TaskLifecycle as _, TaskReportOwner as _};

use super::{
    fixture::{Case, exercise},
    report_settlement::request,
};

fn blocked(task_id: &str, generation: u64) -> TaskReport {
    TaskReport {
        task: DecisionTaskVersion {
            task_id: task_id.to_owned(),
            generation,
        },
        disposition: TaskDisposition::Blocked,
        decision_call_id: None,
        checkpoint_id: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
    }
}

/// The run's report evidence for `report`, refused, and the code it was refused with.
#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn refused(case: &Case, report: TaskReport) -> Option<String> {
    let captured = Capture::install();
    let prepared = case
        .tasks
        .prepare_report(case.run(), &request(case, Some(report)))
        .await
        .expect("a refusal is answered, not raised");
    assert!(prepared.is_none(), "the evidence prepares no settlement");
    captured
        .only("dt_report_evidence_refused")
        .field("error_code")
        .map(str::to_owned)
}

// Value: protects=a run cannot settle another task with its report; fails_when=
// the run's own task is not compared with the reported one and a sibling task is
// settled; why_new=the other-task arm of execution ownership had no case;
// seam=the production report preparation over live Postgres, two tasks bound and
// the run executing the first.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_naming_a_sibling_task_is_refused_as_stale() {
    exercise(async |case| {
        let bindings = case
            .tasks
            .bind(case.run(), &case.plan("two-goals", 2))
            .await
            .expect("two accepted tasks");
        let sibling = bindings
            .items
            .get(1)
            .and_then(|item| item.task_id.as_ref())
            .expect("the second goal owns its own task");
        let before = case
            .tasks
            .read(&case.scope, sibling)
            .await
            .expect("sibling task");

        let code = refused(case, blocked(sibling.as_str(), 1)).await;

        assert_eq!(code.as_deref(), Some(error_code::TASK_STALE_STATE.as_str()));
        assert_eq!(
            case.tasks
                .read(&case.scope, sibling)
                .await
                .expect("unchanged sibling"),
            before
        );
    })
    .await;
}

// Value: protects=a run cannot report on a generation it is not executing, even
// one naming the task's current generation; fails_when=the run's own generation
// is not compared with the task's, and a paused task is settled by its old run;
// why_new=the generation arm of execution ownership had no case; seam=the
// production report preparation over live Postgres, the task paused under the run.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_on_the_generation_after_a_pause_is_refused_as_stale() {
    exercise(async |case| {
        let bindings = case
            .tasks
            .bind(case.run(), &case.plan("paused-goal", 1))
            .await
            .expect("accepted task");
        let task = bindings
            .items
            .first()
            .and_then(|item| item.task_id.as_ref())
            .expect("the goal owns a task");
        let initial = case
            .tasks
            .read(&case.scope, task)
            .await
            .expect("running task");
        let paused = case
            .tasks
            .control(&case.scope, task, &initial.etag(), 1, TaskControl::Pause)
            .await
            .expect("pause");
        assert_eq!(paused.context.generation, 2);

        let code = refused(case, blocked(task.as_str(), 2)).await;

        assert_eq!(code.as_deref(), Some(error_code::TASK_STALE_STATE.as_str()));
        assert_eq!(
            case.tasks
                .read(&case.scope, task)
                .await
                .expect("paused task"),
            paused
        );
    })
    .await;
}
