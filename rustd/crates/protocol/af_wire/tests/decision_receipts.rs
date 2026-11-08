//! Complete nested receipts preserve typed answers and their independent usage.
#![expect(
    clippy::expect_used,
    reason = "wire scenario prerequisites fail loudly with their decoded payload"
)]

use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome, DecisionResult, DecisionUsage},
    task::{DecisionReceipt, DecisionReceiptInput},
};
use serde_json::{Value, json};

#[test]
fn nested_decision_receipts_preserve_distributions_usage_and_unique_numeric_levels() {
    let document = json!({"fencing_token":7,"receipt":{"outcome":"provider","result":{
        "reported_model":"evaluated-model-1",
        "usage":{"input_tokens":100,"output_tokens":3,"cached_input_tokens":20,"cache_write_tokens":10},
        "result":{"outcome":"answered","answers":{
            "matches":{"type":"noul","noul":0.99},
            "next":{"type":"choice","choice":"WAIT","confidence":0.96,"probabilities":{"WAIT":0.98,"DONE":0.02}},
            "risk":{"type":"score","score":0.2,"confidence":0.8,"probabilities":{"0":0.8,"1":0.2}}
        }}
    }}});
    let expected = complete_receipt();
    let restored: DecisionReceiptInput =
        serde_json::from_value(document.clone()).expect("stored JSON retains numeric score keys");
    assert_eq!(restored, expected);
    let encoded = serde_json::to_vec(&restored).expect("complete transport receipt");
    assert_eq!(
        serde_json::from_slice::<Value>(&encoded).expect("wire JSON"),
        document
    );
    assert_eq!(
        serde_json::from_slice::<DecisionReceiptInput>(&encoded).expect("nested wire receipt"),
        expected
    );
    malformed_score_keys_are_rejected(&document);
    assert_eq!(
        serde_json::to_value(restored).expect("retained receipt"),
        document
    );
}

fn malformed_score_keys_are_rejected(document: &Value) {
    let distributions = ["not-a-level", "-1", "0.5", "4294967296"]
        .into_iter()
        .map(|key| json!({(key):1.0}))
        .chain([
            json!({"0":0.8,"00":0.6,"1":0.4}),
            json!({"0":0.8,"+0":0.6,"1":0.4}),
        ]);
    for probabilities in distributions {
        let mut invalid = document.clone();
        *invalid
            .pointer_mut("/receipt/result/result/answers/risk/probabilities")
            .expect("score distribution") = probabilities;
        *invalid
            .pointer_mut("/receipt/result/result/answers/risk/score")
            .expect("score expectation") = json!(0.4);
        let bytes = serde_json::to_vec(&invalid).expect("complete invalid receipt");
        let fault = serde_json::from_value::<DecisionReceiptInput>(invalid)
            .expect_err("invalid or colliding integral keys cannot replace another level");
        let wire = serde_json::from_slice::<DecisionReceiptInput>(&bytes)
            .expect_err("raw receipt enforces the same numeric identities");
        assert_eq!(
            (fault.classify(), wire.classify()),
            (
                serde_json::error::Category::Data,
                serde_json::error::Category::Data
            )
        );
    }
}

fn complete_receipt() -> DecisionReceiptInput {
    DecisionReceiptInput {
        fencing_token: 7,
        receipt: DecisionReceipt::Provider {
            result: DecisionResult {
                reported_model: Some("evaluated-model-1".to_owned()),
                usage: Some(DecisionUsage {
                    input_tokens: 100,
                    output_tokens: 3,
                    cached_input_tokens: 20,
                    cache_write_tokens: 10,
                }),
                result: DecisionOutcome::Answered {
                    answers: [
                        ("matches".to_owned(), DecisionAnswer::Noul { noul: 0.99 }),
                        (
                            "next".to_owned(),
                            DecisionAnswer::Choice {
                                choice: "WAIT".to_owned(),
                                confidence: Some(0.96),
                                probabilities: [
                                    ("WAIT".to_owned(), 0.98),
                                    ("DONE".to_owned(), 0.02),
                                ]
                                .into(),
                            },
                        ),
                        (
                            "risk".to_owned(),
                            DecisionAnswer::Score {
                                score: 0.2,
                                confidence: Some(0.8),
                                probabilities: [(0, 0.8), (1, 0.2)].into(),
                            },
                        ),
                    ]
                    .into(),
                },
            },
        },
    }
}
