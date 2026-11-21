//! Two requests under one operation key that both pass every replay check meet
//! at the receipt insert: the one that loses it replays the winner's result.
//!
//! Each proof holds the receipt table in SHARE mode, which reads still pass and
//! every receipt write waits on, so both requests queue at their reservation
//! before either files a receipt. That stalls every receipt writer in the
//! database, so each proof owns its database.

use super::{
    fixture::{Fixture, now},
    mint,
};
use sqlx::Connection as _;

/// Gates every receipt write behind the holder.
const GATE: &str = "LOCK TABLE setup.replay_receipts IN SHARE MODE";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_creates_queued_at_the_receipt_make_one_draft() {
    let f = Fixture::isolated().await;
    let operation = mint();
    let mut holder = f.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *held)
        .await
        .expect("holder session");
    sqlx::query(GATE)
        .execute(&mut *held)
        .await
        .expect("gate receipt writes");
    let create = || {
        f.drafts.create(
            f.access(),
            f.request(operation.clone(), Some("Investigate checkout")),
            now(),
        )
    };
    let (left, right, ()) = tokio::join!(create(), create(), async {
        f.waits_on(pid, 2).await;
        held.rollback().await.expect("open the gate");
    });
    drop(holder);
    let (left, right) = (
        left.expect("one create lands"),
        right.expect("the other replays it"),
    );
    assert_eq!(left.draft.id, right.draft.id);
    let replays = [left, right].map(|created| created.accepted.expect("message").replayed);
    assert_eq!(
        replays.iter().filter(|replayed| **replayed).count(),
        1,
        "exactly the loser replays: {replays:?}"
    );
    assert_eq!(f.counts().await, (1, 1, 1));
    f.cleanup().await;
}
