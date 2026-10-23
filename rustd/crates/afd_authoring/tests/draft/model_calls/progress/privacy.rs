//! Current authority gates every later frame and never blocks behind the provider.

use super::fixture::{Live, WAIT};
use crate::draft::{attempt::Attempt, fixture::Fixture, model_calls};
use afd_authoring::types::Source;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy)]
enum Revoke {
    Stop,
    Delete,
    Membership,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn stop_withholds_later_preview_and_retains_known_usage() {
    revoked(Revoke::Stop).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn deletion_withholds_later_preview_and_cannot_recreate_private_text() {
    revoked(Revoke::Delete).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn membership_revocation_withholds_later_preview_but_usage_still_settles() {
    revoked(Revoke::Membership).await;
}

async fn revoked(revoke: Revoke) {
    let f = Fixture::create().await;
    let access = if matches!(revoke, Revoke::Membership) {
        f.member_access()
    } else {
        f.access()
    };
    let attempt = Attempt::start(&f, access, Source::Blank).await;
    let mut live = Live::start(&f, &attempt).await;
    let task = live.call(&f.runner, &attempt, model_calls::fixture::request(&attempt));
    live.arrived().await;
    live.text("authorized answer").await;
    assert_eq!(live.next().await.text, "authorized answer");
    tokio::time::timeout(WAIT, mutate(&f, &attempt, revoke))
        .await
        .expect("authority changes while provider remains open");
    live.text("must never escape").await;
    live.finish().await;
    tokio::time::timeout(WAIT, task)
        .await
        .expect("late settlement")
        .expect("dispatch task")
        .expect_err("publication authority is gone");
    live.drained().await;
    settled_without_private_copy(&f, &attempt).await;
    assert_eq!(live.provider.calls.load(Ordering::SeqCst), 1);
    f.cleanup().await;
}

async fn settled_without_private_copy(f: &Fixture, attempt: &Attempt) {
    let receipt: (bool, String) = sqlx::query_as("SELECT result IS NULL AND native_reply IS NULL,usage->>'knowledge' FROM authoring.dispatches WHERE attempt_id=$1::uuid")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("accounting without private copy");
    assert_eq!(receipt, (true, "known".into()));
    assert_eq!(
        model_calls::fixture::receipts(f, attempt).await,
        (1, 1, false)
    );
}

async fn mutate(f: &Fixture, attempt: &Attempt, revoke: Revoke) {
    match revoke {
        Revoke::Stop => {
            f.drafts
                .cancel(f.access(), &attempt.draft.id, 1)
                .await
                .expect("Stop");
        }
        Revoke::Delete => {
            f.drafts
                .delete(f.access(), &attempt.draft.id)
                .await
                .expect("delete");
        }
        Revoke::Membership => {
            sqlx::query(
                "DELETE FROM identity.memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid",
            )
            .bind(f.tenant.as_str())
            .bind(f.member.as_str())
            .execute(&mut *f.connection().await)
            .await
            .expect("revoke membership");
        }
    }
}
