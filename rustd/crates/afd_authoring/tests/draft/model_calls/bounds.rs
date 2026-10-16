//! Input/window, cumulative output and wallet refusals leave no physical intent.

use super::{
    super::{attempt::Attempt, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_small_context_window_is_rejected_before_any_dispatch() {
    let f = Fixture::create().await;
    sqlx::query("UPDATE core.model_library SET context_cap_tokens=64 WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("owned model").as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("small supported window");
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, fixture::request(&attempt))
            .await
            .expect_err("assembled tokenizer/window bound")
            .code(),
        error_code::AUTHORING_TOKEN
    );
    assert_eq!(fixture::receipts(&f, &attempt).await, (0, 0, false));
    assert_eq!(provider.count(), 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn spent_output_cannot_create_a_dispatch() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    sqlx::query("UPDATE fleet.authoring_attempts SET output_tokens=8192 WHERE id=$1::uuid")
        .bind(attempt.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("spent output");
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, fixture::request(&attempt))
            .await
            .expect_err("remaining output exhausted")
            .code(),
        error_code::AUTHORING_TOKEN
    );
    assert_eq!(fixture::receipts(&f, &attempt).await, (0, 0, false));
    assert_eq!(provider.count(), 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_missing_wallet_cannot_create_a_dispatch() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    sqlx::query("DELETE FROM billing.tenant_wallet WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("owner wallet missing");
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, fixture::request(&attempt))
            .await
            .expect_err("missing wallet")
            .code(),
        error_code::AUTHORING_BALANCE
    );
    assert_eq!(fixture::receipts(&f, &attempt).await, (0, 0, false));
    assert_eq!(provider.count(), 0);
    f.cleanup().await;
}
