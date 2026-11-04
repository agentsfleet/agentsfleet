//! Complete synthetic provider receipts shared by native storage and HTTP proofs.
#![expect(
    clippy::expect_used,
    reason = "validated fixture questions have nonempty choices and bounded levels"
)]

use afd_wire::{
    decision::{DecisionRequest, DecisionResult},
    task::DecisionReceipt,
};

pub(crate) fn known(request: &DecisionRequest) -> DecisionReceipt {
    use afd_wire::decision::{DecisionAnswer, DecisionOutcome, DecisionQuestion, DecisionUsage};
    let answers = request
        .questions
        .iter()
        .map(|named| {
            let answer = match &named.question {
                DecisionQuestion::Noul { .. } => DecisionAnswer::Noul { noul: 1.0 },
                DecisionQuestion::Choice { options, .. } => DecisionAnswer::Choice {
                    choice: options.first().expect("nonempty options").value.clone(),
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
                    probabilities: levels
                        .iter()
                        .enumerate()
                        .map(|(i, _)| {
                            (
                                u32::try_from(i).expect("bounded levels"),
                                if i == 0 { 1.0 } else { 0.0 },
                            )
                        })
                        .collect(),
                },
            };
            (named.name.clone(), answer)
        })
        .collect();
    DecisionReceipt::Provider {
        result: DecisionResult {
            reported_model: Some("evaluated-fixture-model".to_owned()),
            usage: Some(DecisionUsage {
                input_tokens: 1000,
                cached_input_tokens: 200,
                cache_write_tokens: 0,
                output_tokens: 100,
            }),
            result: DecisionOutcome::Answered { answers },
        },
    }
}
