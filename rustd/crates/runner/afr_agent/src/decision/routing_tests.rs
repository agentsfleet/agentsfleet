//! Reviewed thresholds decide how many requests a message holds and whether a
//! proposed item may start new work.

use super::routing::{Multiplicity, check, multiplicity};
use af_wire::decision::{
    DecisionAnswer, DecisionOutcome, DecisionPurpose, DecisionRequest, DecisionResult,
    DecisionThreshold, IntentDecisionPolicy,
};
use af_wire::durable_task::{
    DecisionCallState, DecisionRecordDisposition, DecisionRecorded, DurableTaskInputAction,
    DurableTaskProposedItem, DurableTaskRequestedTarget,
};
use afr_tools::verbs::DecisionEvaluation;
use std::collections::BTreeMap;

const CALL: &str = "019f0000-0000-7000-8000-000000000001";

fn policy() -> IntentDecisionPolicy {
    let threshold = DecisionThreshold {
        probability_bps: 9000,
        confidence_bps: 9000,
    };
    IntentDecisionPolicy {
        questions_revision: "dt-routing-v1".to_owned(),
        intent: threshold,
        association: threshold,
        single_request_max_bps: 1000,
        multiple_request_min_bps: 9000,
    }
}

fn chosen(choice: &str) -> DecisionAnswer {
    DecisionAnswer::Choice {
        choice: choice.to_owned(),
        confidence: Some(1.0),
        probabilities: BTreeMap::from([(choice.to_owned(), 1.0)]),
    }
}

fn noul(noul: f64) -> BTreeMap<String, DecisionAnswer> {
    BTreeMap::from([(
        "multiple_requests".to_owned(),
        DecisionAnswer::Noul { noul },
    )])
}

/// A known, current evaluation answering `intent` and `association`.
fn evaluation(intent: &str, association: &str) -> DecisionEvaluation {
    let answers = BTreeMap::from([
        ("intent".to_owned(), chosen(intent)),
        ("association".to_owned(), chosen(association)),
    ]);
    DecisionEvaluation {
        call_id: CALL.to_owned(),
        state: DecisionCallState::Known,
        recorded: Some(Box::new(DecisionRecorded {
            request: DecisionRequest {
                call_id: CALL.to_owned(),
                purpose: DecisionPurpose::Intent,
                state: serde_json::json!({}),
                questions: vec![],
            },
            result: DecisionResult {
                reported_model: None,
                usage: None,
                result: DecisionOutcome::Answered { answers },
            },
            disposition: DecisionRecordDisposition::Current,
        })),
    }
}

fn new_work(
    durable_task_id: Option<&str>,
    target: Option<DurableTaskRequestedTarget>,
) -> DurableTaskProposedItem {
    DurableTaskProposedItem {
        source_start: 0,
        source_end: 4,
        action: DurableTaskInputAction::NewWork,
        durable_task_id: durable_task_id.map(str::to_owned),
        title: "Repair the failing build".to_owned(),
        interpretation: "Start the repair as new work".to_owned(),
        target,
    }
}

#[test]
fn a_missing_or_impossible_multiplicity_answer_decides_nothing() {
    assert!(multiplicity(&BTreeMap::new(), &policy()).is_err());
    for impossible in [f64::NAN, -0.1, 1.5] {
        assert!(
            multiplicity(&noul(impossible), &policy()).is_err(),
            "{impossible}"
        );
    }
    assert!(multiplicity(&noul(0.05), &policy()).is_ok_and(|count| count == Multiplicity::Single));
    assert!(
        multiplicity(&noul(0.95), &policy()).is_ok_and(|count| count == Multiplicity::Multiple)
    );
}

#[test]
fn new_work_needs_a_new_association_a_target_and_no_existing_task() -> crate::Result<()> {
    let accepted = evaluation("new_work", "new_work");
    check(
        &accepted,
        &new_work(None, Some(DurableTaskRequestedTarget::ApiOnly)),
        &policy(),
    )?;
    assert!(
        check(&accepted, &new_work(None, None), &policy()).is_err(),
        "new work names its target"
    );
    assert!(
        check(
            &accepted,
            &new_work(Some(CALL), Some(DurableTaskRequestedTarget::ApiOnly)),
            &policy()
        )
        .is_err(),
        "new work never rides an existing task"
    );
    let elsewhere = evaluation("new_work", "task_1");
    assert!(
        check(
            &elsewhere,
            &new_work(None, Some(DurableTaskRequestedTarget::ApiOnly)),
            &policy()
        )
        .is_err(),
        "an association with existing work is not new work"
    );
    Ok(())
}
