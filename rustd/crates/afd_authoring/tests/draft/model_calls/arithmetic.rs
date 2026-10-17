//! Supported integer extremes cannot under-reserve or publish unpriced output.

use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::sync::Arc;

const OVERFLOWING_OUTPUT_TOKENS: u64 = 1_000_000_000;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_unfunded_wallet_cannot_dispatch_an_already_admitted_attempt() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    sqlx::query("UPDATE billing.tenant_wallet SET balance_nanos=0 WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("funding removed after admission");
    let error = calls
        .call(&f.runner, &a.lease, fixture::request(&a))
        .await
        .expect_err("current funding must cover active obligations");
    assert_eq!(error.code(), error_code::AUTHORING_BALANCE);
    assert_eq!(fixture::receipts(&f, &a).await, (0, 0, false));
    assert_eq!(provider.count(), 0);
    assert_eq!(f.counts().await, (1, 0, 1));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn aggregate_reservation_overflow_cannot_authorize_a_physical_request() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    let other = f
        .drafts
        .create(
            f.access(),
            f.request(mint(), Some("Another setup")),
            super::super::fixture::now(),
        )
        .await
        .expect("second owned draft");
    let mut connection = f.connection().await;
    sqlx::query("INSERT INTO fleet.authoring_attempts SELECT (jsonb_populate_record(NULL::fleet.authoring_attempts,to_jsonb(source)||jsonb_build_object('id',$2::text,'lease_id',$3::text,'draft_id',$4::text))).* FROM fleet.authoring_attempts source WHERE id=$1::uuid")
        .bind(a.id.as_str()).bind(mint().as_str()).bind(mint().as_str()).bind(other.draft.id.as_str())
        .execute(&mut *connection).await.expect("second active owned obligation");
    sqlx::query("UPDATE fleet.authoring_attempts SET reserved_nanos=$2 WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .bind(i64::MAX)
        .execute(&mut *connection)
        .await
        .expect("large per-attempt reservations");
    sqlx::query("UPDATE billing.tenant_wallet SET balance_nanos=$2 WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .bind(i64::MAX)
        .execute(&mut *connection)
        .await
        .expect("largest supported wallet");
    drop(connection);
    let error = calls
        .call(&f.runner, &a.lease, fixture::request(&a))
        .await
        .expect_err("total reservation exceeds integer accounting");
    assert_eq!(error.code(), error_code::AUTHORING_BALANCE);
    assert_eq!(fixture::receipts(&f, &a).await, (0, 0, false));
    assert_eq!(provider.count(), 0);
    let obligations: (i64, i64) = sqlx::query_as("SELECT count(*)::bigint,sum(calls)::bigint FROM fleet.authoring_attempts WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str()).fetch_one(&mut *f.connection().await).await.expect("both obligations retained without calls");
    assert_eq!(obligations, (2, 0));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn unrepresentable_provider_cost_is_uncertain_without_private_output_or_redispatch() {
    let f = Fixture::create().await;
    sqlx::query("UPDATE core.model_library SET context_cap_tokens=2147483647,inference=jsonb_set(inference,'{max_output_tokens}','2147483647'::jsonb),output_nanos_per_mtok=$2 WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("owned model").as_str()).bind(i64::MAX)
        .execute(&mut *f.connection().await).await.expect("supported positive model extremes");
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = Provider::known();
    *stub.response.get_mut("usage").expect("usage object") =
        serde_json::json!({"input_tokens":1,"output_tokens":OVERFLOWING_OUTPUT_TOKENS});
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    let mut request = fixture::request(&a);
    request.request.max_output_tokens = 2_000_000_000;
    for request in [request.clone(), request] {
        let error = calls
            .call(&f.runner, &a.lease, request)
            .await
            .expect_err("provider cost cannot fit durable accounting");
        assert_eq!(error.code(), error_code::AUTHORING_UNKNOWN);
    }
    assert_eq!(fixture::receipts(&f, &a).await, (1, 1, true));
    assert_eq!(provider.count(), 1);
    let retained: (bool, String) = sqlx::query_as("SELECT result IS NULL AND native_reply IS NULL,usage->>'knowledge' FROM fleet.authoring_dispatches WHERE attempt_id=$1::uuid")
        .bind(a.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("unknown accounting fences private output");
    assert_eq!(retained, (true, "unknown".into()));
    f.cleanup().await;
}
