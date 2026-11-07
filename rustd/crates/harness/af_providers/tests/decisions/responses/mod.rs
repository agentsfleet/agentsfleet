//! Complete HTTP exchanges reject semantic faults while retaining paid usage.

use af_providers::decision::{DecisionClient, DecisionEvaluator as _};
use af_wire::decision::{
    DecisionApi, DecisionInvalid, DecisionOutcome, DecisionPurpose, DecisionResult, DecisionUsage,
};
use afr_telemetry::{
    labels::decision::{
        DecisionOutcome as MeasuredOutcome, DecisionProvider, DecisionPurpose as MeasuredPurpose,
    },
    testing::{Recorded, Tally, scoped},
};
use serde_json::Value;

use super::fixture::{self, Reply, Server};

mod metadata;
mod native;
mod openai;

type Fault = (&'static str, Value, DecisionOutcome);

async fn exchange(
    api: DecisionApi,
    (name, document, expected): Fault,
    expected_usage: Option<DecisionUsage>,
    purpose: DecisionPurpose,
    reported_model: Option<&str>,
) {
    let mut server = Server::start(vec![Reply::Json(fixture::envelope(api, document))]).await;
    let client = DecisionClient::with_client(
        server.client.clone(),
        fixture::profile(&server, api),
        fixture::credential(),
    );
    let mut request = (*fixture::request()).clone();
    request.purpose = purpose;
    let request = garde::Unvalidated::new(request)
        .validate()
        .expect("same bounded questions in either purpose");
    let (tally, measurements) = Tally::new();
    let result = scoped(tally, client.evaluate(&request))
        .await
        .expect("readable result retains independent accounting");
    let seen = server.next().await;
    let expected = DecisionResult {
        reported_model: reported_model.map(str::to_owned),
        usage: expected_usage,
        result: expected,
    };
    assert_eq!(result, expected, "{api:?}: {name}");
    assert_eq!(seen.body["model"], "requested-model");
    assert_eq!(
        seen.headers["authorization"],
        format!("Bearer {}", fixture::credential().expose())
    );
    assert!(
        !seen
            .body
            .to_string()
            .contains(fixture::credential().expose())
    );
    assert_eq!(
        server.unconsumed(),
        0,
        "semantic faults buy no implicit retry: {name}"
    );
    let measurements: Vec<_> = measurements.try_iter().collect();
    measurements_match(&measurements, api, purpose, &expected.result);
    server.shutdown().await;
}

fn usage(api: DecisionApi) -> DecisionUsage {
    DecisionUsage {
        input_tokens: 100,
        output_tokens: 3,
        cached_input_tokens: if api == DecisionApi::Openai { 20 } else { 0 },
        cache_write_tokens: if api == DecisionApi::Openai { 10 } else { 0 },
    }
}

fn measured_provider(api: DecisionApi) -> DecisionProvider {
    match api {
        DecisionApi::Typesafe => DecisionProvider::Typesafe,
        DecisionApi::Perplexity => DecisionProvider::Perplexity,
        DecisionApi::Cloudflare => DecisionProvider::Cloudflare,
        DecisionApi::Openai => DecisionProvider::Openai,
    }
}

fn measured_outcome(outcome: &DecisionOutcome) -> MeasuredOutcome {
    match outcome {
        DecisionOutcome::Refused => MeasuredOutcome::Refused,
        DecisionOutcome::Invalid {
            reason: DecisionInvalid::Model,
        } => MeasuredOutcome::InvalidModel,
        DecisionOutcome::Invalid {
            reason: DecisionInvalid::Usage,
        } => MeasuredOutcome::InvalidUsage,
        DecisionOutcome::Invalid {
            reason: DecisionInvalid::Answers,
        } => MeasuredOutcome::InvalidAnswers,
        DecisionOutcome::Answered { .. } => MeasuredOutcome::Completed,
    }
}

fn measurements_match(
    measured: &[Recorded],
    api: DecisionApi,
    purpose: DecisionPurpose,
    expected: &DecisionOutcome,
) {
    let measured_purpose = if purpose == DecisionPurpose::Intent {
        MeasuredPurpose::Intent
    } else {
        MeasuredPurpose::Goal
    };
    assert!(
        matches!(measured, [Recorded::Decision(provider, actual_purpose, outcome, _)]
        if *provider == measured_provider(api) && *actual_purpose == measured_purpose && *outcome == measured_outcome(expected))
    );
}

fn invalid() -> DecisionOutcome {
    DecisionOutcome::Invalid {
        reason: DecisionInvalid::Answers,
    }
}
