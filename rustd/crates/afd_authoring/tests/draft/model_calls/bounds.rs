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
async fn a_physical_completion_uses_the_context_left_after_the_assembled_input() {
    let f = Fixture::create().await;
    set_context(&f, 32_768).await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let mut request = fixture::request(&attempt);
    request.request.max_output_tokens = 32_768;
    calls
        .call(&f.runner, &attempt.lease, request)
        .await
        .expect("completion fits remaining context");
    let sent = provider
        .bodies
        .lock()
        .expect("physical request")
        .first()
        .cloned()
        .expect("one request");
    let limit = sent
        .get("max_output_tokens")
        .and_then(serde_json::Value::as_u64)
        .expect("provider ceiling");
    assert!(limit > 0 && limit < 32_768);
    let persisted: i64 = sqlx::query_scalar(
        "SELECT output_allowance FROM fleet.authoring_dispatches WHERE attempt_id=$1::uuid",
    )
    .bind(attempt.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("physical accounting ceiling");
    assert_eq!(persisted.cast_unsigned(), limit);
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn the_complete_request_including_system_instructions_is_bounded_before_dispatch() {
    for context in [64, 2048] {
        let f = Fixture::create().await;
        set_context(&f, context).await;
        let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
        let provider = Arc::new(Provider::known());
        let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
        assert_eq!(
            calls
                .call(&f.runner, &attempt.lease, fixture::request(&attempt))
                .await
                .expect_err("assembled tokenizer/window bound includes system instructions")
                .code(),
            error_code::AUTHORING_TOKEN
        );
        assert_eq!(fixture::receipts(&f, &attempt).await, (0, 0, false));
        assert_eq!(provider.count(), 0);
        f.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn usage_beyond_the_physical_output_or_combined_context_is_uncertain() {
    for (input, output) in [(1, 32_768), (32_768, 1)] {
        let f = Fixture::create().await;
        set_context(&f, 32_768).await;
        let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
        let mut stub = Provider::known();
        *stub.response.get_mut("usage").expect("usage object") =
            serde_json::json!({"input_tokens": input, "output_tokens": output});
        let provider = Arc::new(stub);
        let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
        let mut request = fixture::request(&attempt);
        request.request.max_output_tokens = 32_768;
        assert_eq!(
            calls
                .call(&f.runner, &attempt.lease, request)
                .await
                .expect_err("provider usage exceeded the actual allowance")
                .code(),
            error_code::AUTHORING_UNKNOWN
        );
        assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, true));
        assert_eq!(provider.count(), 1);
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.authoring_dispatches WHERE attempt_id=$1::uuid AND (result IS NOT NULL OR native_reply IS NOT NULL)")
            .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("private output is fenced");
        assert_eq!(visible, 0);
        f.cleanup().await;
    }
}

async fn set_context(f: &Fixture, tokens: i32) {
    sqlx::query("UPDATE core.model_library SET context_cap_tokens=$2,inference=jsonb_set(inference,'{max_output_tokens}',to_jsonb($2::integer)) WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("owned model").as_str()).bind(tokens)
        .execute(&mut *f.connection().await).await.expect("configured context and output ceiling");
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
