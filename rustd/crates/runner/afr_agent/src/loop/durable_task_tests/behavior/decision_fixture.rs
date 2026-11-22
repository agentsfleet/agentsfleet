//! Complete typed question and probability fixtures.

use super::*;

pub(super) fn choice_answer(
    selected: &str,
    confidence: Option<f64>,
    options: &[&str],
) -> DecisionAnswer {
    DecisionAnswer::Choice {
        choice: selected.to_owned(),
        confidence,
        probabilities: options
            .iter()
            .map(|option| {
                (
                    (*option).to_owned(),
                    if *option == selected { 1.0 } else { 0.0 },
                )
            })
            .collect(),
    }
}

pub(super) fn choice_question(name: &str, options: &[&str]) -> NamedQuestion {
    NamedQuestion {
        name: name.to_owned(),
        question: DecisionQuestion::Choice {
            instructions: format!("Choose the supported {name}"),
            options: options
                .iter()
                .map(|option| ChoiceOption {
                    value: (*option).to_owned(),
                    description: format!("Evidence supports {option}"),
                })
                .collect(),
        },
    }
}

/// The reviewed decision profile every task fixture runs under; it reads
/// 24 KiB of state and questions.
pub(super) fn assignment() -> DecisionAssignment {
    let threshold = DecisionThreshold {
        probability_bps: 9000,
        confidence_bps: 9000,
    };
    DecisionAssignment {
        revision_id: SOURCE.to_owned(),
        profile: DecisionProfile {
            model_id: SOURCE.to_owned(),
            revision: "reviewed".to_owned(),
            api: DecisionApi::Typesafe,
            endpoint: "https://decision.example.test/v1".to_owned(),
            model: "decision-model".to_owned(),
            accepted_models: vec!["decision-model".to_owned()],
            capabilities: DecisionCapabilities {
                noul: true,
                choice: true,
                score: false,
            },
            input_max_bytes: 24576,
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
