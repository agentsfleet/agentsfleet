//! Complete synthetic provider receipts, shared by the native storage proofs
//! here and the router's HTTP proofs in `afd_api`.

use af_wire::{
    decision::{DecisionRequest, DecisionResult},
    task::DecisionReceipt,
};

/// A provider receipt answering every question in `request` with its first
/// option, a zero score, or a noul of one.
#[must_use]
pub fn known(request: &DecisionRequest) -> DecisionReceipt {
    DecisionReceipt::Provider {
        result: known_result(request),
    }
}

/// The decision result inside [`known`]'s receipt.
///
/// A choice question with no options answers an empty choice: a validated
/// request always has options, so no fixture reaches that arm.
#[must_use]
pub fn known_result(request: &DecisionRequest) -> DecisionResult {
    use af_wire::decision::{DecisionAnswer, DecisionOutcome, DecisionQuestion, DecisionUsage};
    let answers = request
        .questions
        .iter()
        .map(|named| {
            let answer = match &named.question {
                DecisionQuestion::Noul { .. } => DecisionAnswer::Noul { noul: 1.0 },
                DecisionQuestion::Choice { options, .. } => DecisionAnswer::Choice {
                    choice: options
                        .first()
                        .map(|option| option.value.clone())
                        .unwrap_or_default(),
                    confidence: Some(1.0),
                    probabilities: options
                        .iter()
                        .enumerate()
                        .map(|(i, option)| (option.value.clone(), if i == 0 { 1.0 } else { 0.0 }))
                        .collect(),
                },
                DecisionQuestion::Score { levels, .. } => DecisionAnswer::Score {
                    score: 0.0,
                    confidence: Some(1.0),
                    probabilities: (0_u32..)
                        .zip(levels)
                        .map(|(level, _)| (level, if level == 0 { 1.0 } else { 0.0 }))
                        .collect(),
                },
            };
            (named.name.clone(), answer)
        })
        .collect();
    DecisionResult {
        reported_model: Some("evaluated-fixture-model".to_owned()),
        usage: Some(DecisionUsage {
            input_tokens: 1000,
            cached_input_tokens: 200,
            cache_write_tokens: 0,
            output_tokens: 100,
        }),
        result: DecisionOutcome::Answered { answers },
    }
}
