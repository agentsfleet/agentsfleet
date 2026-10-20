//! The receipt sweep deletes a bounded batch past the window and leaves every
//! message its operation key.

use super::{Fixture, mint, now};
use crate::draft::window::{PAST, message_id, receipts, refile};
use afd_authoring::types::MessageQuery;
use afd_core::timing::DAY;
use std::time::Duration;

/// The most receipts one pass deletes (`afd_authoring`'s `SWEEP_LIMIT`).
const SWEEP_LIMIT: u64 = 64;
/// How many passes a proof runs for a sweep to converge. Every lane's pass
/// sweeps every lane's expired receipts, so a pass may find its batch taken.
const PASSES: usize = 20;

/// Files one expired receipt per id in `$1`, each under its own key.
const EXPIRED: &str = "\
INSERT INTO authoring.replay_receipts
 (id,workspace_id,user_id,scope,operation_id,request_digest,resource_id,created_at)
SELECT r.id::uuid,$3::uuid,$4::uuid,'sweep:'||r.operation,r.operation::uuid,'digest',r.id::uuid,
       statement_timestamp() - $5::interval
  FROM unnest($1::text[],$2::text[]) AS r(id,operation)";

/// This workspace's receipts past the window.
async fn expired(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM authoring.replay_receipts WHERE workspace_id=$1::uuid AND created_at <= statement_timestamp() - $2::interval")
        .bind(f.workspace.as_str())
        .bind(DAY)
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("expired census")
}

/// Runs passes until `count` reads `target`, or [`PASSES`] ran.
async fn sweep_until(f: &Fixture, target: i64, count: impl AsyncFn() -> i64) {
    for _ in 0..PASSES {
        if count().await == target {
            return;
        }
        f.drafts.maintain().await.expect("maintenance pass");
    }
    assert_eq!(count().await, target, "the sweep converged");
}

async fn seed(f: &Fixture, ids: &[String], age: Duration) {
    let operations: Vec<String> = ids.iter().map(|_| mint().to_string()).collect();
    sqlx::query(EXPIRED)
        .bind(ids)
        .bind(&operations)
        .bind(f.workspace.as_str())
        .bind(f.owner.as_str())
        .bind(age)
        .execute(&mut *f.connection().await)
        .await
        .expect("seeded receipts");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_receipt_sweep_is_bounded_and_keeps_receipts_inside_the_window() {
    let f = Fixture::create().await;
    let ids: Vec<String> = (0..=SWEEP_LIMIT).map(|_| mint().to_string()).collect();
    seed(&f, &ids, PAST).await;
    let live = mint().to_string();
    seed(&f, std::slice::from_ref(&live), Duration::ZERO).await;
    let swept = f
        .drafts
        .maintain()
        .await
        .expect("maintenance pass")
        .expired_receipts;
    assert!(swept <= SWEEP_LIMIT, "one pass deleted {swept}");
    sweep_until(&f, 0, async || expired(&f).await).await;
    assert_eq!(
        receipts(&f, &live).await,
        1,
        "a receipt inside the window stays"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_message_past_the_window_keeps_its_operation_id_after_its_receipt_is_swept() {
    let f = Fixture::create().await;
    let key = mint();
    let request = f.request(key.clone(), Some("Watch checkout"));
    let created = f.drafts.create(f.access(), request, now()).await;
    let draft = created.expect("create").draft.id;
    sqlx::query("UPDATE authoring.messages SET created_at = created_at - $2::interval WHERE draft_id=$1::uuid")
        .bind(draft.as_str())
        .bind(PAST)
        .execute(&mut *f.connection().await)
        .await
        .expect("the message is older than the window");
    let message = message_id(&f, &draft, 1).await;
    refile(&f, &key, &message, &f.owner, PAST).await;
    sweep_until(&f, 0, async || receipts(&f, &message).await).await;
    let page = MessageQuery {
        limit: 20,
        starting_after: None,
    };
    let page = f.drafts.messages(f.access(), &draft, page).await;
    let page = page.expect("a message outlives its receipt");
    let user = page.items.first().expect("the user turn");
    assert_eq!((user.role, &user.operation_id), ("user", &key));
    f.cleanup().await;
}
