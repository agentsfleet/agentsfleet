//! Completed replay returns the original result and physical costs are rounded independently.

use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use afd_wire::authoring::{ModelMessage, ModelUsage};
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn exact_model_retry_returns_the_saved_result_without_another_physical_request() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    let first = calls
        .call(&f.runner, &attempt.lease, request.clone())
        .await
        .expect("first physical result");
    assert!(!first.replayed);
    assert_eq!(first.text, "private answer");
    assert_eq!(
        first.usage,
        ModelUsage::Known {
            input_tokens: 1,
            output_tokens: 1,
            provider_cost_nanos: "3000".into()
        }
    );
    let replayed = calls
        .call(&f.runner, &attempt.lease, request.clone())
        .await
        .expect("saved result");
    assert!(replayed.replayed);
    assert_eq!(replayed.text, first.text);
    let mut changed = request;
    changed.request.messages = vec![ModelMessage::User {
        text: "Different private request".into(),
    }];
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, changed)
            .await
            .expect_err("changed body")
            .code(),
        error_code::AUTHORING_REPLAY
    );
    assert_eq!(provider.count(), 1);
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn fractional_nanos_round_per_token_class_and_physical_dispatch() {
    let f = Fixture::create().await;
    sqlx::query("UPDATE model.platform_models SET input_nanos_per_mtok=1,output_nanos_per_mtok=1 WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("first owned model").as_str()).execute(&mut *f.connection().await).await.expect("fractional rate");
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    let first = fixture::request(&attempt);
    let mut next = first.clone();
    next.dispatch_id = mint().to_string();
    next.call_index = 2;
    for request in [first, next] {
        let result = calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect("priced dispatch");
        assert!(
            matches!(result.usage,ModelUsage::Known {provider_cost_nanos,..} if provider_cost_nanos=="2")
        );
    }
    let cost: i64 = sqlx::query_scalar("SELECT sum((usage->>'provider_cost_nanos')::bigint)::bigint FROM authoring.dispatches WHERE attempt_id=$1::uuid")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("physical cost receipts");
    assert_eq!(cost, 4);
    assert_eq!(fixture::receipts(&f, &attempt).await, (2, 2, false));
    assert_eq!(provider.count(), 2);
    f.cleanup().await;
}
