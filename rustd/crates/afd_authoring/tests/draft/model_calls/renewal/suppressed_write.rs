//! A successful statement that changed no row cannot acknowledge a renewal.

use super::super::fixture::{self, Provider};
use super::{Attempt, Fixture, request, snapshot};
use crate::draft::refusal::Refusal;
use afd_authoring::types::Source;
use afd_core::error_code;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_ignored_expiry_write_cannot_acknowledge_or_meter_a_renewal() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    let before = snapshot(&f, &a).await;
    let fault = Refusal::install(
        &f.owner_url(),
        "authoring.attempts",
        "ignore_renewal",
        a.id.as_str(),
        &format!(
            "NEW.id='{}'::uuid AND NEW.lease_expires_at<>OLD.lease_expires_at",
            a.id
        ),
        "RETURN NULL",
    )
    .await;
    let error = f
        .drafts
        .renew(&f.runner, &a.lease, request())
        .await
        .expect_err("no expiry was written");
    assert_eq!(error.code(), error_code::AUTHORING_FENCE);
    assert_eq!(snapshot(&f, &a).await, before);
    assert_eq!(fixture::receipts(&f, &a).await, (0, 0, false));
    drop(fault);
    let renewed = f
        .drafts
        .renew(&f.runner, &a.lease, request())
        .await
        .expect("same renewal succeeds once the boundary recovers");
    let after = snapshot(&f, &a).await;
    assert_eq!(after.0, renewed.expires_at);
    assert_ne!(after.0, before.0);
    assert_eq!((after.1, after.2), (before.1, before.2));
    f.cleanup().await;
}
