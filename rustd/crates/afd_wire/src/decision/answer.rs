//! Common answer invariants are independent of any model's threshold policy.

use std::collections::BTreeMap;

use super::{DecisionAnswer, DecisionQuestion};

/// Provider-rounded probability totals may differ by at most one ten-thousandth.
const ROUNDING_TOLERANCE: f64 = 0.0001;

/// Checks the complete typed distribution against its accepted question.
/// This validates evidence shape; it grants no authority or completion.
#[must_use]
pub fn answer_matches(question: &DecisionQuestion, answer: &DecisionAnswer) -> bool {
    match (question, answer) {
        (DecisionQuestion::Noul { .. }, DecisionAnswer::Noul { noul }) => probability(*noul),
        (
            DecisionQuestion::Choice { options, .. },
            DecisionAnswer::Choice {
                choice,
                confidence,
                probabilities,
            },
        ) => {
            let Some(selected) = probabilities.get(choice) else {
                return false;
            };
            probabilities.len() == options.len()
                && options
                    .iter()
                    .all(|option| probabilities.contains_key(&option.value))
                && distribution(probabilities.values().copied())
                && probabilities.values().all(|value| value <= selected)
                && confidence.is_none_or(probability)
        }
        (
            DecisionQuestion::Score { levels, .. },
            DecisionAnswer::Score {
                score,
                confidence,
                probabilities,
            },
        ) => {
            score_distribution(levels.len(), *score, probabilities)
                && confidence.is_none_or(probability)
        }
        _ => false,
    }
}

fn probability(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn distribution(probabilities: impl Iterator<Item = f64>) -> bool {
    let mut total = 0.0;
    for value in probabilities {
        if !probability(value) {
            return false;
        }
        total += value;
    }
    (total - 1.0).abs() <= ROUNDING_TOLERANCE
}

fn score_distribution(levels: usize, score: f64, probabilities: &BTreeMap<u32, f64>) -> bool {
    if probabilities.len() != levels
        || !score.is_finite()
        || !distribution(probabilities.values().copied())
    {
        return false;
    }
    let mut expected = 0.0;
    for ((&index, &probability), expected_index) in probabilities.iter().zip(0_u32..) {
        if index != expected_index {
            return false;
        }
        expected += f64::from(index) * probability;
    }
    (score - expected).abs() <= ROUNDING_TOLERANCE
}
