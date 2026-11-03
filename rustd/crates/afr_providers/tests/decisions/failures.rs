//! Fault scenarios verify bounded sends, retained usage and diagnostic outcomes.

use std::error::Error as _;
use std::time::Duration;

use afd_wire::decision::{DecisionApi, DecisionInvalid, DecisionOutcome};
use afr_providers::decision::{DecisionClient, DecisionEvaluator as _};
use afr_telemetry::{
    labels::decision::{DecisionOutcome as MeasuredOutcome, DecisionPurpose},
    testing::{Recorded, Tally, scoped},
};
use serde_json::json;

use super::fixture::{self, Reply, Server};

#[tokio::test]
async fn physical_failures_are_bounded_and_never_retried_implicitly() {
    let mut exact = exact_response_succeeds().await;
    exact.push(b' ');
    let cases = vec![
        (Reply::Bytes(exact), MeasuredOutcome::Oversize),
        (
            Reply::Chunks(vec![vec![b' '; 40 * 1024], vec![b' '; 40 * 1024]]),
            MeasuredOutcome::Oversize,
        ),
        (Reply::Bytes(b"{".to_vec()), MeasuredOutcome::Malformed),
        (Reply::Broken, MeasuredOutcome::Unknown),
        (Reply::Status(429, Some("1")), MeasuredOutcome::RateLimited),
        (Reply::Status(529, Some("1")), MeasuredOutcome::RateLimited),
        (
            Reply::PrivateFailure("private-evidence-never-log".to_owned()),
            MeasuredOutcome::ProviderFailure,
        ),
    ];
    for (reply, expected) in cases {
        let mut server = Server::start(vec![reply]).await;
        let (tally, measurements) = Tally::new();
        let result = scoped(
            tally,
            DecisionClient::with_client(
                server.client.clone(),
                fixture::profile(&server, DecisionApi::Typesafe),
                fixture::credential(),
            )
            .evaluate(&fixture::request()),
        )
        .await;
        let error = result.expect_err("failed physical call is not an answer");
        assert!(!error.decision_not_sent());
        assert!(!format!("{error:?}").contains("private-evidence-never-log"));
        server.next().await;
        assert_eq!(
            server.unconsumed(),
            0,
            "one physical reservation permits one attempt"
        );
        let measured: Vec<_> = measurements.try_iter().collect();
        assert!(
            matches!(measured.as_slice(), [Recorded::Decision(_, DecisionPurpose::Goal, actual, _)] if *actual == expected)
        );
        if expected == MeasuredOutcome::RateLimited {
            assert!(matches!(error.decision_status(), Some(429 | 529)));
            assert_eq!(error.decision_retry_after(), Some(Duration::from_secs(1)));
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_missing_result_times_out_and_preserves_unknown_delivery() {
    let mut server = Server::start(vec![Reply::Withheld]).await;
    let profile = fixture::profile(&server, DecisionApi::Typesafe);
    let (tally, measurements) = Tally::new();
    let request = fixture::request();
    let credential = fixture::credential();
    let client = DecisionClient::with_client(server.client.clone(), profile, credential);
    let call = scoped(tally, client.evaluate(&request));
    tokio::pin!(call);
    tokio::select! {
        _ = server.next() => {},
        result = &mut call => panic!("withheld call must await its response: {result:?}"),
    }
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(13)).await;
    let error = tokio::time::timeout(Duration::from_secs(1), &mut call)
        .await
        .expect("physical deadline is bounded")
        .expect_err("missing result cannot become success");
    assert!(!error.decision_not_sent());
    assert!(
        error.source().is_some(),
        "transport cause survives the error shell"
    );
    assert!(matches!(
        measurements.try_iter().collect::<Vec<_>>().as_slice(),
        [Recorded::Decision(_, _, MeasuredOutcome::Timeout, _)]
    ));
    assert_eq!(
        server.unconsumed(),
        0,
        "unknown delivery is never retried automatically"
    );
    tokio::time::resume();
    server.shutdown().await;
}

#[tokio::test]
async fn redirects_do_not_transfer_evidence_or_credentials() {
    let mut other = Server::start(vec![Reply::Json(fixture::native_reply())]).await;
    let mut server = Server::start(vec![Reply::Redirect(format!("{}/stolen", other.base))]).await;
    let error = DecisionClient::with_client(
        server.client.clone(),
        fixture::profile(&server, DecisionApi::Typesafe),
        fixture::credential(),
    )
    .evaluate(&fixture::request())
    .await
    .expect_err("redirect is a refusal");
    assert!(!error.decision_not_sent());
    server.next().await;
    assert_eq!(other.connections(), 0);
    assert_eq!(other.unconsumed(), 0);
    server.shutdown().await;
    other.shutdown().await;
}

#[tokio::test]
async fn missing_usage_and_provider_refusal_remain_separate_from_answers() {
    missing_usage_is_reported().await;
    cloudflare_failure_retains_known_usage().await;

    let answers = json!([
        {"name":"matches", "type":"predicate", "probability":0.99},
        {"name":"matches", "type":"predicate", "probability":0.99},
        {"name":"risk", "type":"score", "score":0.2, "confidence":0.8, "probabilities":[{"value":0,"label":"0","probability":0.8},{"value":1,"label":"1","probability":0.2}]}
    ]);
    for (answers, expected) in [
        (
            answers,
            DecisionOutcome::Invalid {
                reason: DecisionInvalid::Answers,
            },
        ),
        (
            json!([{"name":"matches","type":"refusal"}, {"name":"next","type":"refusal"}, {"name":"risk","type":"refusal"}]),
            DecisionOutcome::Refused,
        ),
    ] {
        let mut server = Server::start(vec![Reply::Json(json!({"model":"evaluated-model-1","usage":{"input_tokens":100,"output_tokens":3},"answers":answers}))]).await;
        let result = DecisionClient::with_client(
            server.client.clone(),
            fixture::profile(&server, DecisionApi::Openai),
            fixture::credential(),
        )
        .evaluate(&fixture::request())
        .await
        .expect("readable OpenAI response");
        assert_eq!(result.result, expected);
        assert_eq!(
            result
                .usage
                .expect("rejected semantic result still used input")
                .input_tokens,
            100
        );
        server.next().await;
        assert_eq!(server.unconsumed(), 0);
        server.shutdown().await;
    }
}

async fn exact_response_succeeds() -> Vec<u8> {
    let mut exact = serde_json::to_vec(&fixture::native_reply()).expect("fixture JSON");
    exact.resize(64 * 1024, b' ');
    let mut valid = Server::start(vec![Reply::Bytes(exact.clone())]).await;
    assert!(
        DecisionClient::with_client(
            valid.client.clone(),
            fixture::profile(&valid, DecisionApi::Typesafe),
            fixture::credential()
        )
        .evaluate(&fixture::request())
        .await
        .is_ok(),
        "exact response bound is usable"
    );
    valid.next().await;
    valid.shutdown().await;

    exact
}

async fn missing_usage_is_reported() {
    let mut missing_usage = fixture::native_reply();
    missing_usage
        .as_object_mut()
        .expect("response object")
        .remove("usage");
    let mut server = Server::start(vec![Reply::Json(missing_usage)]).await;
    let result = DecisionClient::with_client(
        server.client.clone(),
        fixture::profile(&server, DecisionApi::Typesafe),
        fixture::credential(),
    )
    .evaluate(&fixture::request())
    .await
    .expect("readable response");
    assert_eq!(
        result.result,
        DecisionOutcome::Invalid {
            reason: DecisionInvalid::Usage
        }
    );
    assert!(
        result.usage.is_none(),
        "the caller must retain an unknown usage reservation"
    );
    server.next().await;
    assert_eq!(server.unconsumed(), 0);
    server.shutdown().await;
}

async fn cloudflare_failure_retains_known_usage() {
    let mut server = Server::start(vec![Reply::Json(json!({"success":false, "result":fixture::native_reply(), "errors":[{"message":"private-provider-failure"}]}))]).await;
    let client = DecisionClient::with_client(
        server.client.clone(),
        fixture::profile(&server, DecisionApi::Cloudflare),
        fixture::credential(),
    );
    let result = client
        .evaluate(&fixture::request())
        .await
        .expect("readable provider refusal");
    assert_eq!(result.result, DecisionOutcome::Refused);
    assert_eq!(
        result
            .usage
            .expect("known paid usage is retained")
            .input_tokens,
        100
    );
    assert!(!format!("{result:?}").contains("private-provider-failure"));
    server.next().await;
    assert_eq!(server.unconsumed(), 0);
    server.shutdown().await;
}
