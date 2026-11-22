//! A task's report is prepared only from its own goal decision, answered with
//! the action the report claims at the reviewed thresholds.

use af_core::{
    error_code::{self, ErrorCode},
    test_util::trace::Capture,
};
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome, DecisionResult},
    task::{
        DecisionAssessment, DecisionReceipt, DecisionTaskVersion, TaskDisposition, TaskInputAction,
        TaskReport,
    },
};
use afd_fleet::task::{TaskLifecycle as _, TaskReportOwner as _};

use super::super::{
    fixture::{Case, exercise},
    report_settlement,
};
use super::{
    fixture::{Decisions, request},
    interpretation::{choose, proposed},
    reply::known_result,
};

/// The API-only task an accepted new-work interpretation made current.
async fn accepted(fixture: &Decisions<'_>) -> DecisionTaskVersion {
    let input = garde::Unvalidated::new(proposed(fixture, TaskInputAction::NewWork).await)
        .validate()
        .expect("complete paid interpretation");
    let task = fixture
        .tasks
        .interpret(fixture.case.run(), &input)
        .await
        .expect("an API-only goal")
        .current_task
        .expect("the new goal is the run's task");
    DecisionTaskVersion {
        task_id: task.task_id.to_string(),
        generation: task.generation,
    }
}

/// Records `assessment` for `task` with its provider answers as `edit` leaves
/// them, returning the call's id.
async fn decided(
    fixture: &Decisions<'_>,
    assessment: DecisionAssessment,
    task: &DecisionTaskVersion,
    edit: impl FnOnce(&mut DecisionResult),
) -> String {
    let (_, body) = fixture
        .reserve(&fixture.reservation(assessment, Some(task.clone())))
        .await;
    let request = request(&body);
    let mut result = known_result(&request);
    edit(&mut result);
    fixture
        .record(&request.call_id, DecisionReceipt::Provider { result })
        .await;
    request.call_id
}

fn next_action(result: &mut DecisionResult, action: &str) {
    let answers = match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("the fixture answers every question");
    choose(answers, "next_action", action);
}

fn goal() -> DecisionAssessment {
    DecisionAssessment::Goal {
        action_group_id: afd_db::test_util::mint_id().into(),
    }
}

fn report(task: &DecisionTaskVersion, disposition: TaskDisposition, call: String) -> TaskReport {
    TaskReport {
        task: task.clone(),
        disposition,
        decision_call_id: Some(call),
        checkpoint_id: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
    }
}

/// `semantic` prepares no report, and the refusal logged says `code`.
#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn refused(fixture: &Decisions<'_>, semantic: TaskReport, code: ErrorCode) {
    let case = fixture.case;
    let capture = Capture::install();
    let prepared = fixture
        .tasks
        .prepare_report(
            case.run(),
            &report_settlement::request(case, Some(semantic)),
        )
        .await
        .expect("a refusal is answered, not raised");
    assert!(prepared.is_none(), "the claim is not prepared");
    let event = capture.only("dt_report_evidence_refused");
    assert_eq!(event.field("error_code"), Some(code.as_str()));
}

// Value: protects=a completion is prepared only when the goal decision says DONE
// and every accepted statement clears its floor; fails_when=the next-action
// comparison or the statement floor is dropped; why_new=both refusals had no
// case; seam=the production reserve, receipt and report-preparation verbs over
// live Postgres; no_claim=settlement after a prepared report.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_completion_the_goal_decision_does_not_support_is_ambiguous() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let task = accepted(&fixture).await;
        let waiting = decided(&fixture, goal(), &task, |result| {
            next_action(result, "WAIT");
        })
        .await;
        refused(
            &fixture,
            report(&task, TaskDisposition::Complete, waiting),
            error_code::TASK_INPUT_AMBIGUOUS,
        )
        .await;
        let unproved = decided(&fixture, goal(), &task, |result| {
            next_action(result, "DONE");
            if let DecisionOutcome::Answered { answers } = &mut result.result {
                for answer in answers.values_mut() {
                    if let DecisionAnswer::Noul { noul } = answer {
                        *noul = 0.5;
                    }
                }
            }
        })
        .await;
        refused(
            &fixture,
            report(&task, TaskDisposition::Complete, unproved),
            error_code::TASK_INPUT_AMBIGUOUS,
        )
        .await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

// Value: protects=only a goal decision about the task's current execution
// supports a report; fails_when=the assessment-kind check is dropped and an item
// decision stands in for the goal; why_new=the check had no case; seam=the
// production reserve, receipt and report-preparation verbs over live Postgres,
// the item decision recorded while the task is current; no_claim=a stale stamp.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_backed_by_a_non_goal_decision_is_stale() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let task = accepted(&fixture).await;
        let item = DecisionAssessment::Item {
            ordinal: 0,
            interpretation_digest: "a".repeat(64),
            source_start: 0,
            source_end: 6,
        };
        let call = decided(&fixture, item, &task, |_| {}).await;
        refused(
            &fixture,
            report(&task, TaskDisposition::Complete, call),
            error_code::TASK_STALE_STATE,
        )
        .await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

// Value: protects=a continuation backed by a goal decision that says CONTINUE is
// prepared; fails_when=the continue threshold is not read, or a decision-backed
// continuation is refused; why_new=the decision-backed continue arm had no case;
// seam=the production reserve, receipt and report-preparation verbs over live
// Postgres; no_claim=the continuation turn the settlement queues.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_continuation_the_goal_decision_supports_is_prepared() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let task = accepted(&fixture).await;
        let call = decided(&fixture, goal(), &task, |result| {
            next_action(result, "CONTINUE");
        })
        .await;
        let semantic = report(&task, TaskDisposition::Continue, call);
        let prepared = fixture
            .tasks
            .prepare_report(
                case.run(),
                &report_settlement::request(case, Some(semantic)),
            )
            .await
            .expect("a supported continuation prepares");
        assert!(prepared.is_some(), "the continuation is prepared");
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
