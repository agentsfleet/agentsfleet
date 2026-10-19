//! A queued chat is re-proved under the host lock: a creator who is gone, a
//! draft past its retention and a draft row that cannot be read admit nothing.

use super::fixture::{attempts, fund, funded, queue};
use super::{Fixture, mint, now};
use afd_core::error_code;
use sqlx::Connection as _;

/// Holds the fixture host the way admission locks it first.
const LOCK_HOST: &str = "SELECT id FROM runner.runners WHERE id=$1::uuid FOR UPDATE";

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_creator_removed_while_the_claim_waits_on_the_host_admits_nothing() {
    let f = funded(1).await;
    let draft = f
        .drafts
        .create(
            f.member_access(),
            f.request(mint(), Some("Queued by a member")),
            now(),
        )
        .await
        .expect("a member's queued chat")
        .draft;
    let mut holder = f.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *held)
        .await
        .expect("holder session");
    sqlx::query(LOCK_HOST)
        .bind(f.runner.as_str())
        .execute(&mut *held)
        .await
        .expect("hold the host");
    let (placed, ()) = tokio::join!(f.drafts.claim_queued(&f.runner, &draft.id), async {
        f.waits_on(pid, 1).await;
        sqlx::query("DELETE FROM identity.users WHERE id=$1::uuid")
            .bind(f.member.as_str())
            .execute(&mut *f.connection().await)
            .await
            .expect("the member's account is removed");
        held.rollback().await.expect("release the host");
    });
    drop(holder);
    assert!(placed.expect("admission answers").is_none());
    assert_eq!(attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_draft_past_its_retention_admits_nothing() {
    // Its own database: every lane's maintenance pass purges expired drafts.
    let f = fund(Fixture::isolated().await, 1).await;
    let draft = queue(&f, "Queued, then retained past its window").await;
    sqlx::query(
        "UPDATE authoring.drafts SET expires_at=now()-interval '1 second' WHERE id=$1::uuid",
    )
    .bind(draft.id.as_str())
    .execute(&mut *f.connection().await)
    .await
    .expect("the retention window closed");
    let placed = f
        .drafts
        .claim_queued(&f.runner, &draft.id)
        .await
        .expect("admission answers");
    assert!(placed.is_none());
    assert_eq!(attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_draft_row_in_no_known_state_refuses_the_claim_as_a_stored_fault() {
    // Its own database: no other proof's sweep may meet the unreadable row.
    let f = fund(Fixture::isolated().await, 1).await;
    let draft = queue(&f, "Queued under a state this daemon never wrote").await;
    sqlx::query("UPDATE authoring.drafts SET status='archived' WHERE id=$1::uuid")
        .bind(draft.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("another writer's state word");
    let refused = f
        .drafts
        .claim_queued(&f.runner, &draft.id)
        .await
        .expect_err("an unreadable draft is no skip");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(attempts(&f).await, 0);
    f.cleanup().await;
}
