#![expect(
    clippy::expect_used,
    reason = "test fixtures and extractor assertions should fail loudly"
)]

use afd_auth::principal::{Principal, Runner};
use afd_core::id::Uuid7;
use axum::extract::FromRequestParts as _;

use super::RunnerIdentity;

fn runner() -> Runner {
    let id =
        Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011").expect("fixture identifier is UUIDv7");
    Runner::new(id, false)
}

#[tokio::test]
async fn runner_identity_extracts_only_a_proven_runner() {
    let (mut parts, _body) = http::Request::new(()).into_parts();
    parts.extensions.insert(Principal::Runner(runner()));
    let extracted = RunnerIdentity::from_request_parts(&mut parts, &())
        .await
        .expect("runner principal extracts");
    assert!(!extracted.0.is_degraded());

    let (mut missing, _body) = http::Request::new(()).into_parts();
    let response = RunnerIdentity::from_request_parts(&mut missing, &())
        .await
        .expect_err("a handler without its guard fails closed");
    assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
}
