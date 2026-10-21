//! A queued chat is re-proved under the host lock: a creator who is gone, a
//! draft past its retention and a draft row that cannot be read admit nothing.

use super::fixture::{attempts, fund, funded, queue};
use super::{Fixture, mint, now};
use afd_core::error_code;
use sqlx::Connection as _;

/// Holds the fixture host the way admission locks it first.
const LOCK_HOST: &str = "SELECT id FROM runner.runners WHERE id=$1::uuid FOR UPDATE";

/// Longer than any unblocked claim takes on a loaded lane.
const PROMPT: std::time::Duration = std::time::Duration::from_secs(10);

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

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_claim_chosen_as_a_deadlock_victim_surfaces_the_datastore_error() {
    // Its own database: the cycle runs through the catalogue's one row, which
    // every lane's model reads share.
    let f = fund(Fixture::isolated().await, 1).await;
    let draft = queue(&f, "Claimed into a lock cycle").await;
    // The claim reads its context, and the catalogue with it, before any
    // lock; the cycle has to meet the model read it repeats under the host
    // lock. So one session holds the host until the claim queues on it, and a
    // second takes the catalogue before the host is let go.
    let mut host = f.connection().await;
    let mut host_held = host.begin().await.expect("host holder");
    let host_pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM runner.runners WHERE id=$1::uuid FOR UPDATE",
    )
    .bind(f.runner.as_str())
    .fetch_one(&mut *host_held)
    .await
    .expect("hold the host");
    let mut catalogue = sqlx::PgConnection::connect(&f.owner_url())
        .await
        .expect("the lane owner connects");
    let mut catalogue_held = catalogue.begin().await.expect("catalogue holder");
    let (placed, ()) = tokio::join!(f.drafts.claim_queued(&f.runner, &draft.id), async {
        f.waits_on(host_pid, 1).await;
        let catalogue_pid: i32 =
            sqlx::query_scalar("SELECT pg_backend_pid() FROM model.catalogue_version FOR UPDATE")
                .fetch_one(&mut *catalogue_held)
                .await
                .expect("hold the catalogue");
        host_held
            .commit()
            .await
            .expect("let the claim take the host");
        // The claim holds the host and waits on the catalogue; asking for the
        // host closes the cycle, and the claim, which waited first, is the
        // one Postgres aborts.
        f.waits_on(catalogue_pid, 1).await;
        sqlx::query(LOCK_HOST)
            .bind(f.runner.as_str())
            .execute(&mut *catalogue_held)
            .await
            .expect("the host frees once the claim is aborted");
        catalogue_held.rollback().await.expect("release both");
    });
    drop(host);
    drop(catalogue);
    let refused = placed.expect_err("a deadlock victim admits nothing");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_claim_skips_a_creator_another_transaction_holds_rather_than_waiting() {
    let f = funded(1).await;
    let draft = queue(&f, "Queued by a creator whose account is busy").await;
    let mut holder = f.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    sqlx::query("SELECT id FROM identity.users WHERE id=$1::uuid FOR UPDATE")
        .bind(f.owner.as_str())
        .fetch_one(&mut *held)
        .await
        .expect("hold the creator's account");
    // Called while the account is held: a claim that waited could not answer.
    let placed = tokio::time::timeout(PROMPT, f.drafts.claim_queued(&f.runner, &draft.id))
        .await
        .expect("the claim answered while the account was held")
        .expect("a skipped creator is no error");
    assert!(placed.is_none(), "a held creator is skipped, not admitted");
    held.rollback().await.expect("release the account");
    // Handed back before cleanup: closing the pool waits for every checkout.
    drop(holder);
    assert!(
        super::fixture::claim(&f, &draft).await.is_some(),
        "admitted once free"
    );
    f.cleanup().await;
}
