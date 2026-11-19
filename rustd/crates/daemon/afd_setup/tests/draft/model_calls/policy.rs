//! A new replica's larger admission defaults cannot enlarge an accepted attempt.

use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    fixture::{self, Provider},
};
use afd_setup::types::Source;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn saved_call_cap_allows_receipt_replay_but_refuses_another_physical_call() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query("UPDATE setup.attempts SET max_model_calls=1 WHERE id=$1")
        .bind(&attempt.id)
        .execute(&mut *f.connection().await)
        .await
        .expect("accepted one-call cap");
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    let first = calls
        .call(&f.runner, &attempt.run, request.clone())
        .await
        .expect("accepted first call");
    assert!(!first.replayed);
    let replay = calls
        .call(&f.runner, &attempt.run, request.clone())
        .await
        .expect("saved receipt");
    assert!(replay.replayed);
    let mut next = request;
    next.dispatch_id = mint().to_string();
    next.call_index = 2;
    let failure = calls
        .call(&f.runner, &attempt.run, next)
        .await
        .expect_err("accepted call cap");
    assert!(failure.to_string().contains("bound"));
    assert_eq!(provider.count(), 1);
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_unreadable_saved_policy_refuses_before_a_provider_effect() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query("UPDATE setup.attempts SET max_model_calls=9 WHERE id=$1")
        .bind(&attempt.id)
        .execute(&mut *f.connection().await)
        .await
        .expect("corrupt owned snapshot");
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let failure = calls
        .call(&f.runner, &attempt.run, fixture::request(&attempt))
        .await
        .expect_err("unreadable accepted policy");
    assert_eq!(failure.code(), af_core::error_code::INTERNAL_DB_QUERY);
    assert_eq!(provider.count(), 0);
    assert_eq!(fixture::receipts(&f, &attempt).await, (0, 0, false));
    f.cleanup().await;
}
