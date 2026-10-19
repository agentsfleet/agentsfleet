//! A valid renewal keeps its original capability; equality and regression fail.

use super::super::fixture::{self, Provider};
use super::{Attempt, EXPIRE_NOW, Fixture, request, snapshot};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::sync::Arc;

/// Leaves the attempt ten seconds live from database time: long enough to
/// renew, short enough that a renewal visibly extends it.
const SHORTEN: &str = "UPDATE authoring.attempts \
SET lease_expires_at=date_trunc('milliseconds',clock_timestamp())+interval '10 seconds' \
WHERE id=$1::uuid";

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn renewal_extends_only_a_live_slot_and_keeps_its_capability_binding() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    sqlx::query(SHORTEN)
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("short live lease");
    let before = snapshot(&f, &a).await;
    let renewed = f
        .drafts
        .renew(&f.runner, &a.lease, request())
        .await
        .expect("live guarded renewal");
    assert!(renewed.expires_at > before.0 && !renewed.stop_requested);
    assert_eq!(renewed.model_access_expires_at, renewed.expires_at);
    assert_eq!(renewed.remaining_output_tokens, 8192);
    assert_eq!(snapshot(&f, &a).await.2, before.2);
    sqlx::query(EXPIRE_NOW)
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("expire at equality");
    let expired = snapshot(&f, &a).await;
    assert_eq!(
        f.drafts
            .renew(&f.runner, &a.lease, request())
            .await
            .expect_err("no revival")
            .code(),
        error_code::AUTHORING_FENCE
    );
    assert_eq!(snapshot(&f, &a).await, expired);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_renewal_whose_hold_the_biller_cannot_answer_extends_nothing_and_fails_nothing() {
    use crate::draft::biller::{Faults, Faulty, Reserve};
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    let offline = afd_authoring::Drafts::new(
        f.db.clone(),
        afd_crypto::entropy::Entropy::new(),
        Faulty::over(
            &f.db,
            Faults {
                reserve: Reserve::Down,
                ..Faults::NONE
            },
        ),
    );
    let before = snapshot(&f, &a).await;
    let refused = offline
        .renew(&f.runner, &a.lease, request())
        .await
        .expect_err("an unanswered hold is no extension");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert_eq!(snapshot(&f, &a).await, before, "the lease is not extended");
    let failure: Option<String> =
        sqlx::query_scalar("SELECT failure_reason FROM authoring.attempts WHERE id=$1::uuid")
            .bind(a.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("the attempt");
    assert_eq!(failure, None, "an outage is not the owner's refusal");
    f.cleanup().await;
}
