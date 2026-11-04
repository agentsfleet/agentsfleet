//! Full HTTP scenarios preserve accounting when semantic evidence is unusable.

use afd_wire::decision::{DecisionApi, DecisionInvalid, DecisionOutcome, DecisionPurpose};
use serde_json::{Value, json};

use super::{exchange, fixture, usage};
use fixture::replaced;

type MetadataFault = (
    &'static str,
    &'static str,
    Value,
    DecisionInvalid,
    bool,
    Option<&'static str>,
);

#[tokio::test]
async fn every_protocol_keeps_model_identity_and_unknown_accounting_in_intent_results() {
    for api in [
        DecisionApi::Typesafe,
        DecisionApi::Perplexity,
        DecisionApi::Cloudflare,
        DecisionApi::Openai,
    ] {
        let original = if api == DecisionApi::Openai {
            fixture::openai_reply()
        } else {
            fixture::native_reply()
        };
        for (name, path, replacement, reason, known, reported_model) in metadata_faults() {
            let fault = (
                name,
                replaced(&original, path, replacement),
                DecisionOutcome::Invalid { reason },
            );
            exchange(
                api,
                fault,
                known.then(|| usage(api)),
                DecisionPurpose::Intent,
                reported_model,
            )
            .await;
        }
    }
}

fn metadata_faults() -> [MetadataFault; 5] {
    [
        (
            "unreviewed model",
            "/model",
            json!("unreviewed-model"),
            DecisionInvalid::Model,
            true,
            Some("unreviewed-model"),
        ),
        (
            "missing model",
            "/model",
            Value::Null,
            DecisionInvalid::Model,
            true,
            None,
        ),
        (
            "missing usage",
            "/usage",
            Value::Null,
            DecisionInvalid::Usage,
            false,
            Some("evaluated-model-1"),
        ),
        (
            "negative usage",
            "/usage/input_tokens",
            json!(-1),
            DecisionInvalid::Usage,
            false,
            Some("evaluated-model-1"),
        ),
        (
            "cache exceeds input",
            "/usage",
            json!({"input_tokens":10,"output_tokens":3,"cached_input_tokens":11}),
            DecisionInvalid::Usage,
            false,
            Some("evaluated-model-1"),
        ),
    ]
}
