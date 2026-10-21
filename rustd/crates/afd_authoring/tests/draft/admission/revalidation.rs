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
    let mut holder = sqlx::PgConnection::connect(&f.owner_url())
        .await
        .expect("the lane owner connects");
    let mut held = holder.begin().await.expect("holder transaction");
    let pid: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM model.catalogue_version FOR UPDATE")
            .fetch_one(&mut *held)
            .await
            .expect("hold the catalogue");
    let (placed, ()) = tokio::join!(f.drafts.claim_queued(&f.runner, &draft.id), async {
        // The claim holds the host and waits on the catalogue its model read
        // shares; asking for the host closes the cycle, and the claim, which
        // waited first, is the one Postgres aborts.
        f.waits_on(pid, 1).await;
        sqlx::query(LOCK_HOST)
            .bind(f.runner.as_str())
            .execute(&mut *held)
            .await
            .expect("the host frees once the claim is aborted");
        held.rollback().await.expect("release both");
    });
    drop(holder);
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

/// Runs `statement` as the isolated lane's owner: privileges are the owner's to
/// change, and they live in this database alone.
async fn as_owner(f: &Fixture, statement: &'static str) {
    let mut owner = sqlx::PgConnection::connect(&f.owner_url())
        .await
        .expect("the lane owner connects");
    sqlx::query(statement)
        .execute(&mut owner)
        .await
        .expect("the owner changes the API role's privileges");
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_claim_whose_non_waiting_lock_fails_otherwise_surfaces_the_datastore_error() {
    let f = fund(Fixture::isolated().await, 1).await;
    let draft = queue(&f, "Locked under a privilege the API lost").await;
    // `FOR UPDATE` needs UPDATE: the draft's NOWAIT now fails as a permission
    // error, not as a held row, and a failure is no reason to skip.
    as_owner(&f, "REVOKE UPDATE ON authoring.drafts FROM api_runtime").await;
    let refused = f.drafts.claim_queued(&f.runner, &draft.id).await;
    as_owner(&f, "GRANT UPDATE ON authoring.drafts TO api_runtime").await;
    let refused = refused.expect_err("a lock that failed is not a lock that was held");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_claim_whose_access_read_fails_surfaces_the_datastore_error() {
    let f = fund(Fixture::isolated().await, 1).await;
    let draft = queue(&f, "Authorized through a table the API cannot read").await;
    as_owner(&f, "REVOKE SELECT ON identity.workspaces FROM api_runtime").await;
    let refused = f.drafts.claim_queued(&f.runner, &draft.id).await;
    as_owner(&f, "GRANT SELECT ON identity.workspaces TO api_runtime").await;
    let refused = refused.expect_err("an unreadable membership is no skip");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(attempts(&f).await, 0);
    f.cleanup().await;
}
