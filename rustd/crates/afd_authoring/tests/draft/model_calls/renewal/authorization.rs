//! Renewal rechecks host authorization, selected settings and reserved money.

use super::super::fixture::{self, Provider};
use super::{Attempt, Fixture, request, snapshot};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::sync::Arc;

// Value: protects=an unusable model cannot prolong a lease or lose its recovery action;
// fails_when=renewal keeps a stale model or hides the saved daemon failure;
// why_new=existing renewal proofs exercise only time and wallet lock contention;
// seam=real Postgres; no_claim=live runner renewal.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn missing_or_invalid_model_settings_deny_renewal_with_durable_recovery() {
    for inference in [
        "null",
        r#"{"api":"responses","base_url":"https://models.example.com/v1","max_output_tokens":200000}"#,
    ] {
        let f = Fixture::create().await;
        let a = Attempt::start(&f, f.access(), Source::Blank).await;
        let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
        let before = snapshot(&f, &a).await;
        sqlx::query("UPDATE model.platform_models SET inference=NULLIF($2::jsonb,'null'::jsonb) WHERE id=$1::uuid")
            .bind(f.catalogue.first().expect("owned catalogue model").as_str()).bind(inference).execute(&mut *f.connection().await).await.expect("changed saved model");
        let error = f
            .drafts
            .renew(&f.runner, &a.lease, request())
            .await
            .expect_err("model no longer usable");
        assert_eq!(error.code(), error_code::AUTHORING_MODEL);
        assert_eq!(error.current_state(), Some("unavailable"));
        assert_eq!(snapshot(&f, &a).await, before);
        let saved: (String, String) = sqlx::query_as(
            "SELECT status,failure_reason FROM authoring.messages WHERE draft_id=$1::uuid",
        )
        .bind(a.draft.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("saved model failure");
        assert_eq!(saved, ("failed".into(), "model_unavailable".into()));
        f.cleanup().await;
    }
}

// Value: protects=work cannot renew beyond its reserved wallet coverage;
// fails_when=the reservation is trusted without recomputing the saved model bound;
// why_new=previous renewal always had its full reservation;
// seam=real Postgres and current credentials; no_claim=live billing.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_underreserved_attempt_cannot_extend_its_lease() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    let before = snapshot(&f, &a).await;
    sqlx::query("UPDATE authoring.attempts SET reserved_nanos=1 WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("insufficient own reservation");
    let error = f
        .drafts
        .renew(&f.runner, &a.lease, request())
        .await
        .expect_err("reservation no longer covers work");
    assert_eq!(error.code(), error_code::AUTHORING_BALANCE);
    assert_eq!(snapshot(&f, &a).await, before);
    let reason: String =
        sqlx::query_scalar("SELECT failure_reason FROM authoring.attempts WHERE id=$1::uuid")
            .bind(a.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("saved owner billing failure");
    assert_eq!(reason, "insufficient_balance");
    f.cleanup().await;
}

// Value: protects=administratively removed hosts lose renewal authority;
// fails_when=a removed host keeps extending an otherwise valid private lease;
// why_new=existing renewal tests keep the runner active;
// seam=real Postgres; no_claim=live runner management.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_removed_host_cannot_extend_or_meter_a_private_lease() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    let before = snapshot(&f, &a).await;
    sqlx::query("UPDATE runner.runners SET status='removed' WHERE id=$1::uuid")
        .bind(f.runner.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("removed runner");
    assert_eq!(
        f.drafts
            .renew(&f.runner, &a.lease, request())
            .await
            .expect_err("host not authorized")
            .code(),
        error_code::AUTHORING_FENCE
    );
    assert_eq!(snapshot(&f, &a).await, before);
    f.cleanup().await;
}
