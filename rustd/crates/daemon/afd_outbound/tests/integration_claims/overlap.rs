//! Two claims of one lane that overlap in time: the second starts while the
//! first has written its claim and not committed it.

use afd_db::test_util::mint_id;
use afd_outbound::obligation;

use crate::{Lane, Ledger, owe_uncommitted};

/// Rows of `lane` whose claim is live right now.
async fn live_claims(ledger: &Ledger, lane: &Lane) -> i64 {
    let mut connection = ledger.database.acquire().await.expect("a connection");
    sqlx::query_scalar(
        "SELECT count(*) FROM fleet.obligations \
         WHERE workspace_id = $1 AND claim_expires_at > clock_timestamp()",
    )
    .bind(&lane.workspace)
    .fetch_one(&mut *connection)
    .await
    .expect("the live claims count")
}

/// The earlier row of a lane commits after one claim chose the later row and
/// before that claim committed. A second claim starting in that gap sees the
/// earlier row as the head and the later row's claim as not yet written, so a
/// claim that guards only the rows it picks would take both rows at once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_claim_beside_an_uncommitted_claim_takes_nothing_from_its_lane()
-> Result<(), &'static str> {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let (early, early_event) = owe_uncommitted(&ledger, &lane, "early").await;
    let late_event = ledger.owe(&lane, "late").await;

    let connection = ledger.database.acquire().await.expect("a connection");
    let mut first = sqlx::Transaction::begin(connection, None)
        .await
        .expect("the first claim's transaction");
    let held = obligation::claim_in(&mut first, 8, &mint_id())
        .await
        .expect("the first claim");
    let [held] = held.as_slice() else {
        return Err("the first claim takes the committed row");
    };
    assert_eq!(held.outgoing.event_id, late_event);
    early.commit().await.expect("the earlier report commits");

    let beside = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the overlapping claim");
    let taken: Vec<_> = beside.iter().map(|job| &job.outgoing.event_id).collect();
    assert!(
        taken.is_empty(),
        "a lane whose claim is still being written offers nothing, took {taken:?} \
         (the earlier row is {early_event})"
    );
    first.commit().await.expect("the first claim commits");

    let after = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the claim after both");
    assert!(after.is_empty(), "the committed claim holds the lane");
    assert_eq!(
        live_claims(&ledger, &lane).await,
        1,
        "one holder in the lane"
    );
    ledger.cleanup().await;
    Ok(())
}
