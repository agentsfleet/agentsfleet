//! A claim never waits on a row another transaction holds: a busy draft is
//! skipped for the next one, and a held wallet is not a lock the claim takes.
//!
//! Each proof calls the claim WHILE its holder still holds the row and
//! releases only after the claim has answered. A claim that waited could
//! finish only after that release, so it would never finish; [`PROMPT`] turns
//! that hang into a failure instead of a stuck suite.

use super::fixture::{attempts, fund, queue};
use super::{Fixture, mint, now};
use afd_authoring::Claimed;
use afd_authoring::types::Draft;
use afd_crypto::entropy::Entropy;
use afd_runner::Runners;
use afd_wire::lease::AcceptedKinds;
use sqlx::pool::PoolConnection;
use std::time::Duration;

/// Longer than any unblocked claim takes on a loaded lane; a claim still
/// running then is waiting on the held row.
const PROMPT: Duration = Duration::from_secs(10);

/// Workers the host offers: enough for every chat a proof queues.
const WORKERS: i32 = 4;

/// Locks one draft row until its transaction ends.
const HOLD_DRAFT: &str = "SELECT id FROM authoring.drafts WHERE id=$1::uuid FOR UPDATE";

/// Locks a tenant's wallet row, the way a ledger write does.
const HOLD_WALLET: &str =
    "SELECT balance_nanos FROM billing.wallets WHERE tenant_id=$1::uuid FOR UPDATE";

/// One offered claim on the fixture's host, failing the proof if it waits.
async fn claim(f: &Fixture) -> Option<Claimed> {
    let ticket = Runners::new(f.db.clone(), Entropy::new())
        .claim_offer(&f.runner, AcceptedKinds::ALL)
        .await
        .expect("the host answers")
        .expect("an active host is offered");
    tokio::time::timeout(PROMPT, f.drafts.claim_offered(&f.runner, ticket))
        .await
        .expect("the claim answered while the row was held, so it did not wait")
        .expect("admission answers")
}

/// A chat queued by the workspace member, a second creator beside the owner.
async fn queue_as_member(f: &Fixture, text: &str) -> Draft {
    f.drafts
        .create(f.member_access(), f.request(mint(), Some(text)), now())
        .await
        .expect("queue the member's chat")
        .draft
}

/// Hold `statement`'s row for `id` on a session of its own until committed.
async fn hold(f: &Fixture, statement: &'static str, id: &str) -> PoolConnection<sqlx::Postgres> {
    let mut session = f.connection().await;
    sqlx::query("BEGIN")
        .execute(&mut *session)
        .await
        .expect("open the holder");
    sqlx::query(statement)
        .bind(id)
        .fetch_one(&mut *session)
        .await
        .expect("hold the row");
    session
}

async fn release(mut session: PoolConnection<sqlx::Postgres>) {
    sqlx::query("COMMIT")
        .execute(&mut *session)
        .await
        .expect("release the row");
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn test_locked_draft_does_not_block_other_tenant() {
    // Its own database: an offered claim scans every queued chat.
    let f = fund(Fixture::isolated().await, WORKERS).await;
    let older = queue(&f, "The older chat, whose draft is busy").await;
    let newer = queue_as_member(&f, "The newer chat, from someone else").await;
    let holder = hold(&f, HOLD_DRAFT, older.id.as_str()).await;
    let claimed = claim(&f)
        .await
        .expect("the newer chat is admitted past the busy one");
    assert_eq!(
        claimed.draft_id,
        newer.id.to_string(),
        "the busy draft was skipped"
    );
    release(holder).await;
    let later = claim(&f)
        .await
        .expect("the released draft is admitted on the next poll");
    assert_eq!(later.draft_id, older.id.to_string(), "skipped, not dropped");
    assert_eq!(attempts(&f).await, 2);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn test_all_drafts_locked_returns_no_work() {
    let f = fund(Fixture::isolated().await, WORKERS).await;
    let first = queue(&f, "A busy chat").await;
    let second = queue_as_member(&f, "Another busy chat").await;
    let holders = [
        hold(&f, HOLD_DRAFT, first.id.as_str()).await,
        hold(&f, HOLD_DRAFT, second.id.as_str()).await,
    ];
    assert!(
        claim(&f).await.is_none(),
        "every candidate is held, so no work"
    );
    assert_eq!(attempts(&f).await, 0, "a skipped draft issues nothing");
    for holder in holders {
        release(holder).await;
    }
    assert!(
        claim(&f).await.is_some(),
        "released drafts are claimable again"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn test_locked_wallet_does_not_block_claim() {
    let f = fund(Fixture::isolated().await, WORKERS).await;
    let draft = queue(&f, "A chat whose tenant's wallet is busy").await;
    let holder = hold(&f, HOLD_WALLET, f.tenant.as_str()).await;
    let claimed = claim(&f)
        .await
        .expect("a held wallet does not stop admission");
    assert_eq!(claimed.draft_id, draft.id.to_string());
    release(holder).await;
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn test_stale_context_is_revalidated() {
    let f = fund(Fixture::isolated().await, WORKERS).await;
    let draft = queue(&f, "Read before the lock, changed before it was taken").await;
    let mut holder = f.connection().await;
    sqlx::query("BEGIN")
        .execute(&mut *holder)
        .await
        .expect("open the holder");
    let pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM runner.runners WHERE id=$1::uuid FOR UPDATE",
    )
    .bind(f.runner.as_str())
    .fetch_one(&mut *holder)
    .await
    .expect("hold the host");
    let (placed, ()) = tokio::join!(f.drafts.claim_queued(&f.runner, &draft.id), async {
        // The claim has read its snapshot and now queues on the host; the
        // draft moves while it waits.
        f.waits_on(pid, 1).await;
        sqlx::query(
            "UPDATE authoring.drafts SET resource_version=resource_version+1 WHERE id=$1::uuid",
        )
        .bind(draft.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("the draft changes under the waiting claim");
        sqlx::query("COMMIT")
            .execute(&mut *holder)
            .await
            .expect("release the host");
    });
    drop(holder);
    let placed = placed.expect("a stale snapshot is no error");
    assert!(
        placed.is_none(),
        "not admitted on context read before the change"
    );
    assert_eq!(attempts(&f).await, 0);
    let fresh = super::fixture::claim(&f, &draft).await;
    assert!(fresh.is_some(), "the next poll admits on a fresh read");
    f.cleanup().await;
}
