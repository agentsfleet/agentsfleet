//! Live policy checks precede saved-result reads as well as physical sends.

use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn foreign_host_wrong_capability_and_attempt_binding_create_no_dispatch() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    assert_eq!(
        calls
            .call(&mint(), &attempt.lease, request.clone())
            .await
            .expect_err("foreign runner")
            .code(),
        error_code::AUTHORING_NOT_FOUND
    );
    let mut wrong = request.clone();
    wrong.capability_token = "b".repeat(43);
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, wrong)
            .await
            .expect_err("wrong capability")
            .code(),
        error_code::AUTHORING_FENCE
    );
    let mut wrong = request;
    wrong.attempt_id = mint().to_string();
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, wrong)
            .await
            .expect_err("wrong attempt")
            .code(),
        error_code::AUTHORING_FENCE
    );
    assert_eq!(provider.count(), 0);
    assert_eq!(fixture::receipts(&f, &attempt).await, (0, 0, false));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn credential_revocation_denies_completed_result_replay() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    calls
        .call(&f.runner, &attempt.lease, request.clone())
        .await
        .expect("saved result");
    sqlx::query("DELETE FROM core.tenant_model_entries WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("revoke selected owner model");
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect_err("replay reauthorizes")
            .code(),
        error_code::AUTHORING_MODEL
    );
    assert_eq!(provider.count(), 1);
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    f.cleanup().await;
}
