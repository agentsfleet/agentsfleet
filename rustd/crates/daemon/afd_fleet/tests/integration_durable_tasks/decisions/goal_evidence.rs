//! A task's report is prepared only from its own goal decision at the reviewed
//! thresholds: a completion by its statements, a wait or a continuation by the
//! action it claims.

use af_core::{
    error_code::{self, ErrorCode},
    test_util::trace::Capture,
};
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome, DecisionResult},
    durable_task::{
        DecisionAssessment, DecisionDurableTaskVersion, DecisionReceipt, DurableTaskDisposition,
        DurableTaskReport,
    },
};
use afd_fleet::durable_task::DurableTaskReportOwner as _;

use super::super::{
    fixture::{Case, exercise},
    report_settlement,
};
use super::{
    fixture::{Decisions, request},
    reply::known_result,
    steps::choose,
};

/// An API-only task, bound to the fixture run.
async fn accepted(fixture: &Decisions<'_>) -> DecisionDurableTaskVersion {
    fixture.bind_api_only_goal().await
}

/// Records `assessment` for `task` with its provider answers as `edit` leaves
/// them, returning the call's id.
async fn decided(
    fixture: &Decisions<'_>,
    assessment: DecisionAssessment,
    task: &DecisionDurableTaskVersion,
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

fn report(
    task: &DecisionDurableTaskVersion,
    disposition: DurableTaskDisposition,
    call: String,
) -> DurableTaskReport {
    DurableTaskReport {
        durable_task: task.clone(),
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
async fn refused(fixture: &Decisions<'_>, semantic: DurableTaskReport, code: ErrorCode) {
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

/// The next action a dev goal check gave a finished task: muddled between
/// `DONE` and `BLOCKED`, below every threshold, while the statements held.
fn muddled(result: &mut DecisionResult) {
    let answers = match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("the fixture answers every question");
    answers.insert(
        "next_action".to_owned(),
        DecisionAnswer::Choice {
            choice: "BLOCKED".to_owned(),
            confidence: Some(0.62),
            probabilities: std::collections::BTreeMap::from([
                ("CONTINUE".to_owned(), 0.13),
                ("WAIT".to_owned(), 0.0),
                ("DONE".to_owned(), 0.38),
                ("BLOCKED".to_owned(), 0.49),
            ]),
        },
    );
}

// Value: protects=a completion is prepared when every accepted statement clears
// its floor, whatever the next action says, and refused when one falls short;
// fails_when=the statement floor is dropped or the next action still gates
// completion; why_new=the statements alone now decide done; seam=the production
// reserve, receipt and report-preparation verbs over live Postgres;
// no_claim=settlement after a prepared report.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_completion_is_prepared_by_its_statements_alone() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let task = accepted(&fixture).await;
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
            report(&task, DurableTaskDisposition::Complete, unproved),
            error_code::DURABLE_TASK_INPUT_AMBIGUOUS,
        )
        .await;
        // `known_result` answers every statement at 1.0, above the profile's floor.
        let held = decided(&fixture, goal(), &task, muddled).await;
        let semantic = report(&task, DurableTaskDisposition::Complete, held);
        let prepared = fixture
            .tasks
            .prepare_report(
                case.run(),
                &report_settlement::request(case, Some(semantic)),
            )
            .await
            .expect("a completion its statements support prepares");
        assert!(prepared.is_some(), "the completion is prepared");
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
        let semantic = report(&task, DurableTaskDisposition::Continue, call);
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
