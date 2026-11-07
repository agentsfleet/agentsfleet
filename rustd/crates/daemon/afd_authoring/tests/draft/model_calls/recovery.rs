//! Authoritative late usage resolves accounting without reviving an uncertain attempt.

use super::{
    super::{attempt::Attempt, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::{sync::Arc, time::Duration};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn late_known_usage_does_not_reopen_an_attempt_marked_uncertain() {
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
        .expect("physical effect observed");
    age_intent(&f, &attempt).await;
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request.clone())
            .await
            .expect_err("unfinished old intent")
            .code(),
        error_code::AUTHORING_UNKNOWN
    );
    provider.resume();
    assert_eq!(
        task.await
            .expect("late reply")
            .expect_err("private output remains fenced")
            .code(),
        error_code::AUTHORING_FENCE
    );
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    retained_fence(&f, &attempt).await;
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect_err("known usage cannot revive private capability")
            .code(),
        error_code::AUTHORING_UNKNOWN
    );
    assert_eq!(provider.count(), 1);
    assert_eq!(f.counts().await, (1, 0, 1));
    f.cleanup().await;
}

async fn retained_fence(f: &Fixture, attempt: &Attempt) {
    let state: (String,bool) = sqlx::query_as("SELECT a.status,d.result IS NULL FROM authoring.attempts a JOIN authoring.dispatches d ON d.attempt_id=a.id WHERE a.id=$1::uuid")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("durable fence and accounting");
    assert_eq!(state, ("awaiting_accounting".into(), true));
}

async fn age_intent(f: &Fixture, attempt: &Attempt) {
    sqlx::query("UPDATE authoring.dispatches SET dispatch_expires_at=clock_timestamp()-interval '1 millisecond' WHERE attempt_id=$1::uuid")
        .bind(attempt.id.as_str()).execute(&mut *f.connection().await).await.expect("simulate unreconciled old intent");
}
