//! Full HTTP scenarios preserve accounting when semantic evidence is unusable.

use af_wire::decision::{DecisionApi, DecisionOutcome, DecisionPurpose};
use serde_json::{Value, json};

use super::{Fault, exchange, fixture, invalid, usage};
use fixture::replaced;

#[tokio::test]
async fn native_protocols_reject_wrong_types_missing_names_and_noncanonical_indexes() {
    for api in [
        DecisionApi::Typesafe,
        DecisionApi::Perplexity,
        DecisionApi::Cloudflare,
    ] {
        for fault in native_faults() {
            exchange(
                api,
                fault,
                Some(usage(api)),
                DecisionPurpose::Goal,
                Some("evaluated-model-1"),
            )
            .await;
        }
    }
}

fn native_faults() -> Vec<Fault> {
    let original = fixture::native_reply();
    let mut faults = primitive_faults(&original);
    let mut renamed = original.clone();
    let required = renamed["answers"]
        .as_object_mut()
        .expect("named answers")
        .remove("next")
        .expect("required answer");
    renamed["answers"]["unrequested"] = required;
    faults.extend([
        ("missing requested name", renamed, invalid()),
        (
            "required refusal",
            replaced(&original, "/answers/matches", json!({"type":"refusal"})),
            DecisionOutcome::Refused,
        ),
    ]);
    faults
}

fn primitive_faults(original: &Value) -> Vec<Fault> {
    [
        ("unreadable answer", "/answers/next", json!(false)),
        (
            "wrong primitive",
            "/answers/matches",
            original["answers"]["next"].clone(),
        ),
        (
            "score for predicate",
            "/answers/matches",
            original["answers"]["risk"].clone(),
        ),
        (
            "index alias",
            "/answers/risk/probabilities",
            json!({"00":0.8,"1":0.2}),
        ),
        (
            "nonnumeric index",
            "/answers/risk/legend",
            json!({"low":"Low","1":"High"}),
        ),
        ("unreadable legend", "/answers/risk/legend/0", Value::Null),
        (
            "incomplete score distribution",
            "/answers/risk/probabilities",
            json!({"0":1.0}),
        ),
        (
            "invalid score distribution total",
            "/answers/risk/probabilities",
            json!({"0":0.4,"1":0.4}),
        ),
    ]
    .into_iter()
    .map(|(name, path, replacement)| (name, replaced(original, path, replacement), invalid()))
    .collect()
}
