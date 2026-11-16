//! Real provider responses retain bounded refusal metadata without error-page content.

use super::*;
use crate::recovery::TestHttpsServer;
use af_wire::decision::{DecisionApi, DecisionCapabilities, DecisionProfile, DecisionRequest};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use std::time::Duration;

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const ID: &str = "019f0000-0000-7000-8000-000000000001";

fn client(server: &TestHttpsServer) -> TestResult<DecisionClient> {
    let mut endpoint = url::Url::parse(server.endpoint())?;
    endpoint.set_path("/v1/systemone");
    endpoint.set_query(None);
    let profile = garde::Unvalidated::new(DecisionProfile {
        model_id: ID.to_owned(),
        revision: "reviewed".to_owned(),
        api: DecisionApi::Typesafe,
        endpoint: endpoint.to_string(),
        model: "fixture-model".to_owned(),
        accepted_models: vec!["fixture-model".to_owned()],
        capabilities: DecisionCapabilities {
            noul: true,
            choice: true,
            score: true,
        },
        input_max_bytes: 24576,
    })
    .validate()?;
    Ok(DecisionClient::with_client(
        server.client(),
        profile,
        SecretString::new(["decision", "fixture", "credential"].join("-")),
    ))
}

fn request() -> TestResult<garde::Valid<DecisionRequest>> {
    let request: DecisionRequest = serde_json::from_value(serde_json::json!({
        "call_id":ID,"purpose":"goal","state":{"accepted_request":"Retain the reviewed repair"},
        "questions":[{"name":"matches","question":{"type":"noul","instructions":"Does the evidence match?",
            "criteria":{"true":"It matches","false":"It does not match"}}}]
    }))?;
    Ok(garde::Unvalidated::new(request).validate()?)
}

type StatusCase = (
    u16,
    Option<&'static str>,
    DecisionRetryDelay,
    Option<Duration>,
);

fn statuses() -> [StatusCase; 5] {
    [
        (
            429,
            None,
            DecisionRetryDelay::Unspecified,
            Some(Duration::from_millis(250)),
        ),
        (
            429,
            Some("2"),
            DecisionRetryDelay::WithinBudget { milliseconds: 2000 },
            Some(Duration::from_secs(2)),
        ),
        (
            529,
            Some("0"),
            DecisionRetryDelay::WithinBudget { milliseconds: 0 },
            Some(Duration::ZERO),
        ),
        (529, Some("3"), DecisionRetryDelay::TooLong, None),
        (500, Some("1"), DecisionRetryDelay::Unspecified, None),
    ]
}

#[tokio::test]
async fn provider_status_and_retry_after_have_bounded_logical_retry_metadata() -> TestResult {
    for case in statuses() {
        assert_status(case).await?;
    }
    Ok(())
}

async fn assert_status((status, header, expected, retry): StatusCase) -> TestResult {
    let mut headers = HeaderMap::new();
    if let Some(header) = header {
        headers.insert("retry-after", HeaderValue::from_str(header)?);
    }
    let server = TestHttpsServer::with_headers(
        StatusCode::from_u16(status)?,
        b"private provider reply".to_vec(),
        false,
        headers,
    )
    .await?;
    let result = client(&server)?.evaluate(&request()?).await;
    let Err(failure) = result else {
        return Err("provider refusal was accepted".into());
    };
    assert!(!failure.decision_not_sent());
    assert!(!failure.to_string().contains("private provider reply"));
    let cause = unavailable(&failure);
    assert_eq!(
        cause,
        DecisionUnavailableCause::ProviderStatus {
            status,
            retry_delay: expected
        }
    );
    assert_eq!(retry_delay(&DecisionReceipt::Unknown { cause }), retry);
    server.finish().await?;
    Ok(())
}

#[tokio::test]
async fn an_unusable_success_reply_is_unknown_and_never_qualifies_for_a_status_retry() -> TestResult
{
    let server = TestHttpsServer::with_headers(
        StatusCode::OK,
        b"incomplete private reply".to_vec(),
        false,
        HeaderMap::new(),
    )
    .await?;
    let result = client(&server)?.evaluate(&request()?).await;
    let Err(failure) = result else {
        return Err("unusable provider reply was accepted".into());
    };
    assert!(!failure.decision_not_sent());
    let cause = unavailable(&failure);
    assert_eq!(cause, DecisionUnavailableCause::Transport);
    assert_eq!(retry_delay(&DecisionReceipt::Unknown { cause }), None);
    server.finish().await?;
    Ok(())
}
