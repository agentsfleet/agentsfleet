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
