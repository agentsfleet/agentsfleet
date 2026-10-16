//! Lost acknowledgement preserves the remote effect and its unresolved obligation.

use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::{sync::Arc, time::Duration};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_lost_usage_reply_never_redispatches_and_fences_private_proposals() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = Provider::known();
    *stub.response.get_mut("usage").expect("usage field") = serde_json::Value::Null;
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request.clone())
            .await
            .expect_err("reply lacks authoritative usage")
            .code(),
        error_code::AUTHORING_UNKNOWN
    );
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request.clone())
            .await
            .expect_err("never resend uncertain intent")
            .code(),
        error_code::AUTHORING_UNKNOWN
    );
    let mut next = request;
    next.dispatch_id = mint().to_string();
    next.call_index = 2;
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, next)
            .await
            .expect_err("unknown remaining output")
            .code(),
        error_code::AUTHORING_UNKNOWN
    );
    assert_eq!(
        f.drafts
            .activity(&f.runner, &attempt.lease, attempt.initialize(1))
            .await
            .expect_err("uncertain attempt cannot publish")
            .code(),
        error_code::AUTHORING_FENCE
    );
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, true));
    assert_eq!(provider.count(), 1);
    assert_eq!(f.counts().await, (1, 0, 1));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn competing_dispatch_and_same_intent_retry_cannot_duplicate_an_inflight_effect() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = Provider::known();
    stub.blocked = true;
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    let task = tokio::spawn({
        let (calls, runner, lease, request) = (
            calls.clone(),
            f.runner.clone(),
            attempt.lease.clone(),
            request.clone(),
        );
        async move { calls.call(&runner, &lease, request).await }
    });
    tokio::time::timeout(Duration::from_secs(5), provider.arrived())
        .await
        .expect("physical effect arrived");
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request.clone())
            .await
            .expect_err("inflight retry")
            .code(),
        error_code::AUTHORING_STATE
    );
    let mut next = request;
    next.call_index = 2;
    next.dispatch_id = mint().to_string();
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, next)
            .await
            .expect_err("one inflight call")
            .code(),
        error_code::AUTHORING_STATE
    );
    provider.resume();
    task.await
        .expect("provider task")
        .expect("first effect recorded");
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}
