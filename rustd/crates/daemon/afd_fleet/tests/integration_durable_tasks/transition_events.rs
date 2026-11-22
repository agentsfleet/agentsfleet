//! Each task transition is journalled once, under the state it reached and why.

use std::sync::Arc;

use af_core::test_util::trace::Capture;
use af_wire::durable_task::{
    DecisionDurableTaskVersion, DurableTaskDisposition, DurableTaskReport, DurableTaskState,
};
use afd_fleet::durable_task::{DurableTaskControl, DurableTaskLifecycle as _};

use super::{
    fixture::{Case, exercise, id},
    report_settlement::request,
};

/// The event every task transition is journalled under.
const TRANSITION: &str = "dt_transition";

#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn admitted(
    case: &Case,
) -> (
    DecisionDurableTaskVersion,
    af_core::test_util::trace::CapturedEvent,
) {
    let captured = Capture::install();
    let bindings = case
        .tasks
        .bind(case.run(), &case.plan("journalled-goal", 1))
        .await
        .expect("accepted API-only task");
    let admitted = captured.only(TRANSITION);
    drop(captured);
    let durable_task_id = bindings
        .items
        .first()
        .and_then(|item| item.durable_task_id.as_ref())
        .expect("the goal owns a task")
        .to_string();
    (
        DecisionDurableTaskVersion {
            durable_task_id,
            generation: 1,
        },
        admitted,
    )
}

// Value: protects=an admitted task is journalled with its task, its run and the
// admission reason; fails_when=the admission event drops a field or names
// another run; why_new=the admission transition's fields were never read back;
// seam=the production bind over live Postgres, the journal captured.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_admitted_durable_task_is_journalled_with_its_run_and_reason() {
    exercise(async |case| {
        let (task, admitted) = admitted(case).await;
        assert_eq!(
            admitted.field("durable_task_id"),
            Some(task.durable_task_id.as_str())
        );
        assert_eq!(admitted.field("run_id"), Some(case.held.run_key.as_str()));
        assert_eq!(admitted.field("reason"), Some("admitted"));
    })
    .await;
}

// Value: protects=a person's pause is journalled as the state it requested and
// as human control; fails_when=the control event names the previous state or
// another reason; why_new=the control transition's fields were never read back;
// seam=the production control over live Postgres, the journal captured.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_paused_durable_task_is_journalled_as_human_control() {
    exercise(async |case| {
        let (task, _) = admitted(case).await;
        let durable_task_id = id(&task.durable_task_id);
        let initial = case
            .tasks
            .read(&case.scope, &durable_task_id)
            .await
            .expect("running task");
        let captured = Capture::install();
        let paused = case
            .tasks
            .control(
                &case.scope,
                &durable_task_id,
                &initial.etag(),
                1,
                DurableTaskControl::Pause,
            )
            .await
            .expect("pause");
        let event = captured.only(TRANSITION);
        drop(captured);
        assert_eq!(paused.context.state, DurableTaskState::Paused);
        assert_eq!(
            event.field("durable_task_id"),
            Some(task.durable_task_id.as_str())
        );
        assert_eq!(event.field("state"), Some("paused"));
        assert_eq!(event.field("reason"), Some("human_control"));
    })
    .await;
}

// Value: protects=an input accepted while a run worked blocks that run's
// report from settling the task as current, and the charge still lands;
// fails_when=the report settles a task whose accepted input it never covered, or
// the evidence check stops blocking it; why_new=the changed-evidence arm of
// settlement had no live case; seam=the production report over live Postgres,
// the task carrying one accepted input the report does not cover.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_that_missed_an_accepted_input_blocks_on_changed_evidence() {
    exercise(async |case| {
        let (task, _) = admitted(case).await;
        let durable_task_id = id(&task.durable_task_id);
        let pending = sqlx::query("UPDATE fleet.durable_tasks SET input_sequence=1 WHERE id=$1")
            .bind(&durable_task_id)
            .execute(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("input connection"),
            )
            .await
            .expect("one accepted input the run has not incorporated");
        assert_eq!(pending.rows_affected(), 1);
        let report = request(
            case,
            Some(DurableTaskReport {
                durable_task: task,
                disposition: DurableTaskDisposition::Blocked,
                decision_call_id: None,
                checkpoint_id: None,
                covered_input_sequence: 0,
                effect_watermark: 0,
            }),
        );
        let mut plane = case.held.fixtures.plane();
        plane.durable_task_reports = Some(Arc::new(case.tasks.clone()));

        let captured = Capture::install();
        plane
            .report_with(
                &case.held.runner,
                &report,
                af_core::clock::after(case.held.now, crate::report_seed::SLICE),
                None,
                &mut af_observability::verb_span::VerbClock::untimed(),
            )
            .await
            .expect("the financial report settles");
        let event = captured.only(TRANSITION);
        drop(captured);

        let saved = case
            .tasks
            .read(&case.scope, &durable_task_id)
            .await
            .expect("settled task");
        assert_eq!(saved.context.state, DurableTaskState::Blocked);
        assert_eq!(saved.context.generation, 2);
        assert!(
            saved.context.usage.running_nanos > 0,
            "the run's time is charged"
        );
        assert_eq!(event.field("state"), Some("blocked"));
        assert_eq!(event.field("reason"), Some("goal_evidence_changed"));
        let reason: Option<String> =
            sqlx::query_scalar("SELECT status_reason FROM fleet.durable_tasks WHERE id=$1")
                .bind(&durable_task_id)
                .fetch_one(
                    &mut *case
                        .held
                        .fixtures
                        .database
                        .acquire()
                        .await
                        .expect("reason observer"),
                )
                .await
                .expect("stored reason");
        assert_eq!(reason.as_deref(), Some("goal_evidence_changed"));
        assert!(case.held.fixtures.turn_settled(&case.held.turn_id).await);
    })
    .await;
}

// Value: protects=a report that claims an input boundary or an effect watermark
// the task never reached settles as unverified, not as its claimed disposition;
// fails_when=either claimed cursor stops being compared with the task's own;
// why_new=the claimed-cursor arm of settlement had no case; seam=the production
// report over live Postgres, a blocked disposition claiming one cursor ahead.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_claiming_a_cursor_the_durable_task_never_reached_is_unverified() {
    for (covered_input_sequence, effect_watermark) in [(1, 0), (0, 1)] {
        exercise(async |case| {
            let (task, _) = admitted(case).await;
            let durable_task_id = id(&task.durable_task_id);
            let report = request(
                case,
                Some(DurableTaskReport {
                    durable_task: task,
                    disposition: DurableTaskDisposition::Blocked,
                    decision_call_id: None,
                    checkpoint_id: None,
                    covered_input_sequence,
                    effect_watermark,
                }),
            );
            let mut plane = case.held.fixtures.plane();
            plane.durable_task_reports = Some(Arc::new(case.tasks.clone()));
            plane
                .report_with(
                    &case.held.runner,
                    &report,
                    af_core::clock::after(case.held.now, crate::report_seed::SLICE),
                    None,
                    &mut af_observability::verb_span::VerbClock::untimed(),
                )
                .await
                .expect("the financial report settles");
            let stored: (String, Option<String>) =
                sqlx::query_as("SELECT status,status_reason FROM fleet.durable_tasks WHERE id=$1")
                    .bind(&durable_task_id)
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
                    .expect("stored task");
            assert_eq!(
                stored,
                (
                    "blocked".to_owned(),
                    Some("recovery_or_completion_unverified".to_owned())
                ),
                "claimed input {covered_input_sequence}, watermark {effect_watermark}"
            );
            assert!(case.held.fixtures.turn_settled(&case.held.turn_id).await);
        })
        .await;
    }
}
