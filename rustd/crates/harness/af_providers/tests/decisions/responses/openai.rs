//! Full HTTP scenarios preserve accounting when semantic evidence is unusable.

use af_wire::decision::{DecisionApi, DecisionOutcome, DecisionPurpose};
use serde_json::{Value, json};

use super::{Fault, exchange, fixture, usage};
use fixture::replaced;

#[tokio::test]
async fn openai_rejects_malformed_named_arrays_and_keeps_refusal_distinct() {
    for fault in openai_faults() {
        exchange(
            DecisionApi::Openai,
            fault,
            Some(usage(DecisionApi::Openai)),
            DecisionPurpose::Goal,
            Some("evaluated-model-1"),
        )
        .await;
    }
}

fn openai_faults() -> Vec<Fault> {
    let original = fixture::openai_reply();
    let mut faults: Vec<_> = array_faults(&original)
        .into_iter()
        .map(|(name, path, value)| (name, replaced(&original, path, value), super::invalid()))
        .collect();
    faults.push((
        "required refusal",
        replaced(
            &original,
            "/answers/1",
            json!({"name":"matches","type":"refusal"}),
        ),
        DecisionOutcome::Refused,
    ));
    faults
}

fn array_faults(original: &Value) -> Vec<(&'static str, &'static str, Value)> {
    vec![
        ("unreadable array", "/answers", json!({})),
        ("missing question", "/answers", json!([])),
        ("unknown question", "/answers/1/name", json!("unrequested")),
        (
            "wrong primitive",
            "/answers/2",
            json!({"name":"next","type":"predicate","probability":0.99}),
        ),
        (
            "duplicate name",
            "/answers/2",
            original["answers"][1].clone(),
        ),
        (
            "wrong score label",
            "/answers/0/probabilities/0/label",
            json!("Low"),
        ),
        (
            "duplicate score level",
            "/answers/0/probabilities",
            json!([{"value":0,"label":"0","probability":0.8},{"value":1,"label":"1","probability":0.2},{"value":0,"label":"0","probability":0.8}]),
        ),
        (
            "duplicate choice",
            "/answers/2/probabilities",
            json!([{"value":"WAIT","probability":0.98},{"value":"DONE","probability":0.02},{"value":"WAIT","probability":0.98}]),
        ),
        (
            "negative distribution",
            "/answers/2/probabilities/0/probability",
            json!(-1),
        ),
    ]
}
