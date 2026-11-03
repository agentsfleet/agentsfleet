//! Fault scenarios verify bounded sends, retained usage and diagnostic outcomes.

use afd_crypto::secret::SecretString;
use afd_wire::decision::{DecisionApi, DecisionInvalid, DecisionOutcome};
use afr_providers::decision::{DecisionClient, DecisionEvaluator as _};
use afr_telemetry::{
    labels::decision::{DecisionOutcome as MeasuredOutcome, DecisionPurpose},
    testing::{Recorded, Tally, scoped},
};
use garde::Unvalidated;
use serde_json::{Value, json};

use super::fixture::{self, Reply, Server};

#[tokio::test]
async fn invalid_responses_retain_usage_without_returning_trusted_answers() {
    for (name, document, reason) in invalid_cases() {
        let mut server = Server::start(vec![Reply::Json(document)]).await;
        let result = DecisionClient::with_client(
            server.client.clone(),
            fixture::profile(&server, DecisionApi::Typesafe),
            fixture::credential(),
        )
        .evaluate(&fixture::request())
        .await
        .expect("readable response retains its accounting");
        assert_eq!(result.result, DecisionOutcome::Invalid { reason }, "{name}");
        assert_eq!(
            result
                .usage
                .expect("measured paid result remains available")
                .input_tokens,
            100,
            "{name}"
        );
        server.next().await;
        assert_eq!(
            server.unconsumed(),
            0,
            "invalid local processing must not buy another call: {name}"
        );
        server.shutdown().await;
    }
    super::ambiguity::refuses_ambiguous_documents().await;
}

#[tokio::test]
async fn profile_refusals_cost_no_physical_send() {
    let mut server = Server::start(vec![]).await;
    let original = fixture::profile(&server, DecisionApi::Typesafe);
    for profile in denied_profiles(&original) {
        let profile = Unvalidated::new(profile)
            .validate()
            .expect("profile shape is valid; capability/endpoint judgment is native");
        let (tally, received) = Tally::new();
        let error = scoped(
            tally,
            DecisionClient::with_client(
                server.client.clone(),
                profile.clone(),
                fixture::credential(),
            )
            .evaluate(&fixture::request()),
        )
        .await
        .expect_err("preflight refuses predictably");
        assert!(error.decision_not_sent());
        assert_eq!(
            received
                .try_iter()
                .filter(|record| matches!(
                    record,
                    Recorded::Decision(_, DecisionPurpose::Goal, MeasuredOutcome::NotSent, _)
                ))
                .count(),
            1
        );
    }
    let empty = SecretString::new(String::new());
    assert!(
        DecisionClient::with_client(server.client.clone(), original.clone(), empty.clone())
            .evaluate(&fixture::request())
            .await
            .expect_err("empty credentials are predictable")
            .decision_not_sent()
    );
    let invalid_header = SecretString::new(["private", "header"].join("\n"));
    let error = DecisionClient::with_client(
        server.client.clone(),
        original.clone(),
        invalid_header.clone(),
    )
    .evaluate(&fixture::request())
    .await
    .expect_err("credential syntax is checked before transport");
    assert!(error.decision_not_sent());
    assert!(!format!("{error:?}").contains(invalid_header.expose()));
    assert_eq!(server.connections(), 0);
    assert_eq!(server.unconsumed(), 0);
    server.shutdown().await;
    super::limits::escaped_state_and_complete_questions_keep_their_bounds().await;
}

fn changed(original: &Value, path: &str, replacement: Value) -> Value {
    let mut document = original.clone();
    *document
        .pointer_mut(path)
        .expect("fault path exists in the external fixture") = replacement;
    document
}

fn invalid_cases() -> Vec<(&'static str, Value, DecisionInvalid)> {
    let original = fixture::native_reply();
    let mut cases = vec![
        (
            "unevaluated model",
            changed(&original, "/model", json!("unreviewed-model")),
            DecisionInvalid::Model,
        ),
        (
            "missing required question",
            changed(
                &original,
                "/answers",
                json!({"matches":{"type":"noul","noul":0.99}}),
            ),
            DecisionInvalid::Answers,
        ),
        (
            "changed rubric",
            changed(
                &original,
                "/answers/risk/legend/0",
                json!("Unreviewed meaning"),
            ),
            DecisionInvalid::Answers,
        ),
        (
            "incorrect expected score",
            changed(&original, "/answers/risk/score", json!(0.9)),
            DecisionInvalid::Answers,
        ),
    ];
    cases.extend(probability_faults(&original));
    cases
}

fn probability_faults(original: &Value) -> Vec<(&'static str, Value, DecisionInvalid)> {
    vec![
        (
            "out of range",
            changed(original, "/answers/matches/noul", json!(1.1)),
            DecisionInvalid::Answers,
        ),
        (
            "unknown choice",
            changed(original, "/answers/next/choice", json!("MERGE")),
            DecisionInvalid::Answers,
        ),
        (
            "incomplete distribution",
            changed(original, "/answers/next/probabilities", json!({"WAIT":1.0})),
            DecisionInvalid::Answers,
        ),
        (
            "untrue selected maximum",
            changed(original, "/answers/next/choice", json!("DONE")),
            DecisionInvalid::Answers,
        ),
        (
            "invalid total",
            changed(
                original,
                "/answers/next/probabilities",
                json!({"WAIT":0.5,"DONE":0.3}),
            ),
            DecisionInvalid::Answers,
        ),
    ]
}

fn denied_profiles(
    original: &afd_wire::decision::DecisionProfile,
) -> Vec<afd_wire::decision::DecisionProfile> {
    let mut unsupported = original.clone();
    unsupported.capabilities.score = false;
    let mut too_small = original.clone();
    too_small.input_max_bytes = 1;
    let mut unsafe_url = original.clone();
    "http://private.invalid/v1/systemone".clone_into(&mut unsafe_url.endpoint);
    let mut private_literal = original.clone();
    "https://127.0.0.1/v1/systemone".clone_into(&mut private_literal.endpoint);
    vec![unsupported, too_small, unsafe_url, private_literal]
}
