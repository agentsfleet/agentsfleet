//! An assignment never waits on a row another transaction holds: a busy draft is
//! skipped for the next one, and a held wallet is not a lock the assignment takes.
//!
//! Each proof calls the assignment WHILE its holder still holds the row and
//! releases only after the assignment has answered. An assignment that waited could
//! finish only after that release, so it would never finish; [`PROMPT`] turns
//! that hang into a failure instead of a stuck suite.

use super::fixture::{attempts, fund, queue};
use super::{Fixture, mint, now};
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::run::AcceptedKinds;
use afd_runner::Runners;
use afd_setup::AssignedDraft;
use afd_setup::types::Draft;
use sqlx::pool::PoolConnection;
use std::time::Duration;

/// Longer than any unblocked assignment takes on a loaded lane; an assignment still
/// running then is waiting on the held row.
const PROMPT: Duration = Duration::from_secs(10);

/// Workers the host offers: enough for every chat a proof queues.
const WORKERS: i32 = 4;

/// Locks one draft row until its transaction ends.
const HOLD_DRAFT: &str = "SELECT id FROM setup.drafts WHERE id=$1 FOR UPDATE";

/// Locks a tenant's wallet row, the way a ledger write does.
const HOLD_WALLET: &str = "SELECT balance_nanos FROM billing.wallets WHERE tenant_id=$1 FOR UPDATE";

/// One assignment on the fixture's host, failing the proof if it waits.
async fn assign(f: &Fixture) -> Option<AssignedDraft> {
    let ticket = Runners::new(f.db.clone(), Entropy::new())
        .assignment_ticket(&f.runner, AcceptedKinds::ALL)
        .await
        .expect("the host answers")
        .expect("an active host is assigned");
    tokio::time::timeout(PROMPT, f.drafts.assign_next_draft(&f.runner, ticket))
        .await
        .expect("the assignment answered while the row was held, so it did not wait")
        .expect("assignment answers")
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
async fn hold(f: &Fixture, statement: &'static str, id: &Uuid7) -> PoolConnection<sqlx::Postgres> {
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
    // Its own database: an assignment scans every queued chat.
    let f = fund(Fixture::isolated().await, WORKERS).await;
    let older = queue(&f, "The older chat, whose draft is busy").await;
    let newer = queue_as_member(&f, "The newer chat, from someone else").await;
    let holder = hold(&f, HOLD_DRAFT, &older.id).await;
    let assigned = assign(&f)
        .await
        .expect("the newer chat is admitted past the busy one");
    assert_eq!(
        assigned.draft_id,
        newer.id.to_string(),
        "the busy draft was skipped"
    );
    release(holder).await;
    let later = assign(&f)
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
        hold(&f, HOLD_DRAFT, &first.id).await,
        hold(&f, HOLD_DRAFT, &second.id).await,
    ];
    assert!(
        assign(&f).await.is_none(),
        "every candidate is held, so no work"
    );
    assert_eq!(attempts(&f).await, 0, "a skipped draft issues nothing");
    for holder in holders {
        release(holder).await;
    }
    assert!(
        assign(&f).await.is_some(),
        "released drafts are assignable again"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn test_locked_wallet_does_not_block_assignment() {
    let f = fund(Fixture::isolated().await, WORKERS).await;
    let draft = queue(&f, "A chat whose tenant's wallet is busy").await;
    let holder = hold(&f, HOLD_WALLET, &f.tenant).await;
    let assigned = assign(&f)
        .await
        .expect("a held wallet does not stop assignment");
    assert_eq!(assigned.draft_id, draft.id.to_string());
    release(holder).await;
    f.cleanup().await;
}
