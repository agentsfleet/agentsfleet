//! A request the decision model cannot read whole is refused as too large,
//! apart from a decision that is unavailable, before any reservation.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on a request the profile accepts"
)]

use af_core::error_code;
use af_wire::decision::{
    ChoiceOption, DecisionApi, DecisionAssignment, DecisionCapabilities, DecisionProfile,
    DecisionPurpose, DecisionQuestion, DecisionRequest, DecisionThreshold, GoalDecisionPolicy,
    IntentDecisionPolicy, NamedQuestion,
};

use super::supported;
use crate::{error::ErrorKind, task::TaskRefusal};

const ID: &str = "019f0000-0000-7000-8000-000000000001";

fn assignment(input_max_bytes: u32, choice: bool) -> DecisionAssignment {
    let threshold = DecisionThreshold {
        probability_bps: 9000,
        confidence_bps: 9000,
    };
    DecisionAssignment {
        revision_id: ID.to_owned(),
        profile: DecisionProfile {
            model_id: ID.to_owned(),
            revision: "reviewed".to_owned(),
            api: DecisionApi::Typesafe,
            endpoint: "https://decision.example.test/v1".to_owned(),
            model: "decision-model".to_owned(),
            accepted_models: vec!["decision-model".to_owned()],
            capabilities: DecisionCapabilities {
                noul: true,
                choice,
                score: false,
            },
            input_max_bytes,
        },
        intent: IntentDecisionPolicy {
            questions_revision: "dt-routing-v1".to_owned(),
            intent: threshold,
            association: threshold,
            single_request_max_bps: 1000,
            multiple_request_min_bps: 9000,
        },
        goal: GoalDecisionPolicy {
            questions_revision: "dt-goal-v1".to_owned(),
            statement_min_bps: 9000,
            continue_action: threshold,
            wait: threshold,
            done: threshold,
        },
    }
}

fn request(evidence_bytes: usize) -> DecisionRequest {
    DecisionRequest {
        call_id: ID.to_owned(),
        purpose: DecisionPurpose::Goal,
        state: serde_json::json!({ "evidence": "e".repeat(evidence_bytes) }),
        questions: vec![NamedQuestion {
            name: "next_action".to_owned(),
            question: DecisionQuestion::Choice {
                instructions: "Choose the supported next action".to_owned(),
                options: ["DONE", "WAIT", "CONTINUE"]
                    .map(|value| ChoiceOption {
                        value: value.to_owned(),
                        description: format!("Evidence supports {value}"),
                    })
                    .into(),
            },
        }],
    }
}

fn refusal(assignment: &DecisionAssignment, request: &DecisionRequest) -> Option<TaskRefusal> {
    let failure = supported(assignment, request).err()?;
    match failure.kind() {
        ErrorKind::TaskRefused { reason } => Some(*reason),
        _ => None,
    }
}

#[test]
fn a_request_past_the_models_input_limit_is_refused_as_too_large() {
    let assignment = assignment(8192, true);
    let failure = supported(&assignment, &request(9000))
        .expect_err("state and questions past the profile's input ceiling");
    assert_eq!(failure.code(), error_code::PAYLOAD_TOO_LARGE);
    assert_eq!(
        failure.detail(),
        "the decision request exceeds the decision model's reviewed input limit"
    );
    assert_eq!(
        refusal(&assignment, &request(9000)),
        Some(TaskRefusal::DecisionInputTooLarge)
    );
}

#[test]
fn a_state_past_16_kib_within_the_models_limit_is_supported() {
    supported(&assignment(24576, true), &request(20 * 1024))
        .expect("20 KiB of evidence fits the largest reviewed profile");
}

#[test]
fn a_request_within_the_models_input_limit_is_supported() {
    supported(&assignment(24576, true), &request(12 * 1024))
        .expect("12 KiB of evidence fits the largest reviewed profile");
}

#[test]
fn a_question_the_profile_cannot_answer_stays_unavailable_whatever_its_size() {
    for evidence_bytes in [16, 9000] {
        assert_eq!(
            refusal(&assignment(8192, false), &request(evidence_bytes)),
            Some(TaskRefusal::DecisionUnavailable)
        );
    }
}
