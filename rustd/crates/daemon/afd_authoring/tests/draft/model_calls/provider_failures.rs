//! Untrusted model output cannot hide known cost or run an incomplete proposal.

use super::{
    super::{attempt::Attempt, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use afd_wire::authoring::{ProposalCall, ProposalRejection};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_failed_http_status_keeps_known_cost_without_publishing_successful_content() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = Provider::known();
    stub.status = 503;
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    for request in [request.clone(), request] {
        let error = calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect_err("HTTP failure cannot publish the enclosed answer");
        assert_eq!(error.code(), error_code::AUTHORING_PROVIDER);
    }
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    let receipt: (bool, String) = sqlx::query_as("SELECT result IS NULL AND native_reply IS NULL,usage->>'knowledge' FROM authoring.dispatches WHERE attempt_id=$1::uuid")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("retained provider accounting");
    assert_eq!(receipt, (true, "known".into()));
    assert_eq!(provider.count(), 1);
    assert_eq!(f.counts().await, (1, 0, 1));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn malformed_proposal_keeps_known_usage_and_replays_a_nonexecutable_refusal() {
    for arguments in [
        json!({"proposal":{"kind":"grant_access"}}).to_string(),
        "incomplete JSON".into(),
    ] {
        let f = Fixture::create().await;
        let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
        let mut stub = Provider::known();
        *stub.response.get_mut("output").expect("output") = json!([{"type":"function_call","name":"propose_setup","call_id":"call_bad","arguments":arguments}]);
        let provider = Arc::new(stub);
        let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
        let request = fixture::request(&attempt);
        for request in [request.clone(), request] {
            let result = calls
                .call(&f.runner, &attempt.lease, request)
                .await
                .expect("bounded correction feedback");
            assert!(
                matches!(result.proposal_calls.first(), Some(ProposalCall::Refused {
            id, reason: ProposalRejection::InvalidArguments,
        }) if id == "call_bad")
            );
        }
        assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
        let knowledge: String = sqlx::query_scalar(
            "SELECT usage->>'knowledge' FROM authoring.dispatches WHERE attempt_id=$1::uuid",
        )
        .bind(attempt.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("known usage retained");
        assert_eq!(knowledge, "known");
        assert_eq!(provider.count(), 1);
        assert_eq!(attempt.activity_count(&f).await, 0);
        assert_eq!(f.counts().await, (1, 0, 1));
        f.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn incomplete_proposals_return_no_executable_changes() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = Provider::known();
    *stub.response.get_mut("status").expect("status") = json!("incomplete");
    *stub.response.get_mut("output").expect("output") =
        json!([{"type":"function_call","arguments":"incomplete JSON"}]);
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    let result = calls
        .call(&f.runner, &attempt.lease, fixture::request(&attempt))
        .await
        .expect("bounded cut result with known usage");
    assert!(result.cut);
    assert!(result.proposal_calls.is_empty());
    assert_eq!(f.counts().await, (1, 0, 1));
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn duplicate_proposal_identities_preserve_cost_without_returning_executable_changes() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let proposal = json!({"kind":"initialize","definition":super::super::definition()});
    let arguments = json!({"proposal":proposal}).to_string();
    let item = json!({"type":"function_call","name":"propose_setup","call_id":"same-call","arguments":arguments});
    let mut stub = Provider::known();
    *stub.response.get_mut("output").expect("output") = json!([item.clone(), item]);
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, fixture::request(&attempt))
            .await
            .expect_err("ambiguous proposal identities cannot execute")
            .code(),
        error_code::AUTHORING_PROVIDER
    );
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    assert_eq!(f.counts().await, (1, 0, 1));
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}
