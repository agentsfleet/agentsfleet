//! Revocation and fences precede every mutation and receipt replay.

use super::{Attempt, Fixture, Source, mint};
use afd_core::error_code::{AUTHORING_FENCE, AUTHORING_NOT_FOUND};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn foreign_forged_and_expired_attempts_cannot_publish() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let request = a.initialize(1);
    refuse_foreign_and_forged(&f, &a, &request).await;
    sqlx::query("UPDATE authoring.attempts SET lease_expires_at='epoch' WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("expire owned attempt");
    assert_eq!(
        f.drafts
            .activity(&f.runner, &a.lease, request.clone())
            .await
            .expect_err("expired")
            .code(),
        AUTHORING_FENCE
    );
    assert_eq!(f.counts().await, (1, 0, 1));
    assert_eq!(a.activity_count(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn cancelled_and_deleted_attempts_cannot_replay_private_proposals() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let request = a.initialize(1);
    f.drafts
        .activity(&f.runner, &a.lease, request.clone())
        .await
        .expect("initial proposal");
    f.drafts
        .cancel(f.access(), &a.draft.id, 1)
        .await
        .expect("Stop wins");
    assert_eq!(
        f.drafts
            .activity(&f.runner, &a.lease, request.clone())
            .await
            .expect_err("Stop before replay")
            .code(),
        AUTHORING_FENCE
    );
    f.drafts
        .delete(f.access(), &a.draft.id)
        .await
        .expect("private deletion");
    assert_eq!(
        f.drafts
            .activity(&f.runner, &a.lease, request)
            .await
            .expect_err("deleted private state")
            .code(),
        AUTHORING_NOT_FOUND
    );
    assert_eq!(f.counts().await, (0, 0, 0));
    f.cleanup().await;
}

async fn refuse_foreign_and_forged(f: &Fixture, a: &Attempt, request: &super::ActivityInput) {
    assert_eq!(
        f.drafts
            .activity(&mint(), &a.lease, request.clone())
            .await
            .expect_err("foreign host")
            .code(),
        AUTHORING_NOT_FOUND
    );
    let mut forged = request.clone();
    forged.fence = "2".into();
    assert_eq!(
        f.drafts
            .activity(&f.runner, &a.lease, forged)
            .await
            .expect_err("forged fence")
            .code(),
        AUTHORING_FENCE
    );
}
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn membership_revocation_is_checked_before_proposal_receipt_replay() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.member_access(), Source::Blank).await;
    let request = a.initialize(1);
    f.drafts
        .activity(&f.runner, &a.lease, request.clone())
        .await
        .expect("member proposal");
    sqlx::query("DELETE FROM identity.memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid")
        .bind(f.tenant.as_str())
        .bind(f.member.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("revoke current membership");
    assert_eq!(
        f.drafts
            .activity(&f.runner, &a.lease, request)
            .await
            .expect_err("revocation before replay")
            .code(),
        AUTHORING_NOT_FOUND
    );
    assert_eq!(a.activity_count(&f).await, 1);
    f.cleanup().await;
}
