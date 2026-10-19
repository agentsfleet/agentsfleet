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

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_refusal_while_a_dispatch_is_in_flight_shows_its_usage_as_unknown() {
    use afd_wire::authoring::{Knowledge, Meter, Renew};
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = Provider::known();
    stub.blocked = true;
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let task = tokio::spawn({
        let (runner, lease, request) = (
            f.runner.clone(),
            attempt.lease.clone(),
            fixture::request(&attempt),
        );
        async move { calls.call(&runner, &lease, request).await }
    });
    tokio::time::timeout(Duration::from_secs(5), provider.arrived())
        .await
        .expect("the dispatch reached the provider");
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("the owner removes the model mid-dispatch");
    let renewal = Renew {
        fence: "1".into(),
        usage: Meter {
            knowledge: Knowledge::Known,
            calls: 0,
            input_tokens: Some(0),
            output_tokens: Some(0),
            runtime_ms: 0,
            provider_cost_nanos: None,
        },
    };
    let refused = f
        .drafts
        .renew(&f.runner, &attempt.lease, renewal)
        .await
        .expect_err("the pinned model is gone");
    assert_eq!(refused.code(), error_code::AUTHORING_MODEL);
    let reason: Option<String> =
        sqlx::query_scalar("SELECT failure_reason FROM authoring.messages WHERE draft_id=$1::uuid")
            .bind(attempt.draft.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("the failed message");
    assert_eq!(
        reason.as_deref(),
        Some("provider_usage_unknown"),
        "the in-flight dispatch outranks the model refusal"
    );
    provider.resume();
    task.await
        .expect("late task")
        .expect_err("the late reply cannot publish");
    f.cleanup().await;
}
