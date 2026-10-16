//! Refused renewal cannot change the durable lease or its observed runtime.

use super::super::fixture::{self, Provider};
use super::{Attempt, Fixture, request, snapshot};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_runner_cannot_renew_using_calls_absent_from_daemon_receipts() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    let before = snapshot(&f, &a).await;
    let mut forged = request();
    forged.usage.calls = 1;
    let error = f
        .drafts
        .renew(&f.runner, &a.lease, forged)
        .await
        .expect_err("unreceipted usage refused");
    assert_eq!(error.code(), error_code::INVALID_REQUEST);
    assert_eq!(snapshot(&f, &a).await, before);
    assert_eq!(fixture::receipts(&f, &a).await, (0, 0, false));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_exhausted_saved_allowance_cannot_commit_a_failed_renewal() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    sqlx::query("UPDATE fleet.authoring_attempts SET output_tokens=8193 WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("corrupt exhausted durable allowance");
    let before = snapshot(&f, &a).await;
    let error = f
        .drafts
        .renew(&f.runner, &a.lease, request())
        .await
        .expect_err("negative remaining allowance refused");
    assert_eq!(error.code(), error_code::AUTHORING_TOKEN);
    assert_eq!(snapshot(&f, &a).await, before);
    assert_eq!(fixture::receipts(&f, &a).await, (0, 0, false));
    f.cleanup().await;
}
