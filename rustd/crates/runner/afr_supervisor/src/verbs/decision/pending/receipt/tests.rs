use super::Receipt;
use af_wire::{
    decision::{DecisionOutcome, DecisionResult, DecisionUsage},
    task::{DecisionReceipt, DecisionReceiptInput},
};
use zeroize::Zeroize as _;

#[test]
fn retained_results_hide_and_clear_their_owned_provider_data() -> crate::Result<()> {
    let mut receipt = Receipt::new(DecisionReceiptInput {
        fencing_token: 504,
        receipt: DecisionReceipt::Provider { result: DecisionResult {
            reported_model: Some("private-fixture-model".to_owned()),
            usage: Some(DecisionUsage { input_tokens: 13, output_tokens: 5, cached_input_tokens: 2, cache_write_tokens: 1 }),
            result: serde_json::from_value(serde_json::json!({"outcome":"answered","answers":{
                "private-fixture-question":{"type":"choice","choice":"private-fixture-choice","confidence":0.8,"probabilities":{"private-fixture-choice":1.0}},
                "scored":{"type":"score","score":1.0,"confidence":0.7,"probabilities":{"0":0.0,"1":1.0}},
                "binary":{"type":"noul","noul":0.6}
            }})).map_err(crate::error::encode)?,
        } },
    });
    assert!(!format!("{receipt:?}").contains("private-fixture"));
    let DecisionReceipt::Provider { result } = &receipt.receipt else {
        return Err(crate::error::config("provider result missing"));
    };
    assert_eq!(
        result.usage.as_ref().map(|usage| usage.input_tokens),
        Some(13)
    );
    receipt.zeroize();
    let DecisionReceipt::Provider { result } = &receipt.receipt else {
        return Err(crate::error::config("provider result missing after wiping"));
    };
    assert_eq!(result.reported_model.as_deref(), Some(""));
    assert_eq!(
        result.usage.as_ref().map(|usage| usage.input_tokens),
        Some(0)
    );
    assert!(matches!(&result.result, DecisionOutcome::Answered { answers } if answers.is_empty()));
    assert_eq!(receipt.fencing_token, 0);
    Ok(())
}
