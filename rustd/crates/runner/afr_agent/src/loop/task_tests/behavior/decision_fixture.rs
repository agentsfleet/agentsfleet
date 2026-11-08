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
