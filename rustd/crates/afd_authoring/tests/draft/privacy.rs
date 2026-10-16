//! Account ownership cannot bypass creator privacy; revocation precedes replay.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_core::error_code::{AUTHORING_GONE, AUTHORING_NOT_FOUND};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn workspace_owner_cannot_read_member_draft_and_revocation_blocks_replay() {
    let f = Fixture::create().await;
    let request = f.request(mint(), Some("Private member text"));
    let created = f
        .drafts
        .create(f.member_access(), request.clone(), now())
        .await
        .expect("joined member creates");
    let error = f
        .drafts
        .read(f.access(), &created.draft.id)
        .await
        .expect_err("owner is a different creator");
    assert_eq!(error.code(), AUTHORING_NOT_FOUND);
    sqlx::query("DELETE FROM core.memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid")
        .bind(f.tenant.as_str())
        .bind(f.member.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("commit member revocation");
    let error = f
        .drafts
        .create(f.member_access(), request, now())
        .await
        .expect_err("replay rechecks membership");
    assert_eq!(error.code(), AUTHORING_NOT_FOUND);
    let error = f
        .drafts
        .read(f.member_access(), &created.draft.id)
        .await
        .expect_err("revoked creator cannot reload");
    assert_eq!(error.code(), AUTHORING_NOT_FOUND);
    assert_eq!(
        f.counts().await,
        (1, 1, 1),
        "refusal does not expose or mutate private state"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn deletion_wipes_text_and_tombstones_the_accepted_creation_key() {
    let f = Fixture::create().await;
    let request = f.request(mint(), Some("Text that must disappear"));
    let created = f
        .drafts
        .create(f.access(), request.clone(), now())
        .await
        .expect("native create");
    f.drafts
        .delete(f.access(), &created.draft.id, now())
        .await
        .expect("creator deletes");
    assert_eq!(f.counts().await, (0, 0, 0));
    let error = f
        .drafts
        .create(f.access(), request, now())
        .await
        .expect_err("lost response cannot recreate erased text");
    assert_eq!(error.code(), AUTHORING_GONE);
    let tombstones: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.authoring_replay_receipts \
        WHERE resource_id=$1::uuid AND purged_at IS NOT NULL AND expires_at>purged_at",
    )
    .bind(created.draft.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("retained identity");
    assert_eq!(tombstones, 1);
    f.cleanup().await;
}
