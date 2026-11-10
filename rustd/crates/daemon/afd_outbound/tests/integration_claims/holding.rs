//! What a worker holds: no more claims than its lanes can start, and nothing
//! posted for a row it no longer holds.

use af_core::test_util::trace::Capture;
use afd_db::test_util::mint_id;
use afd_outbound::obligation;
use afd_outbound::{IN_FLIGHT_DELIVERIES, Lanes, Posters};
use tokio_util::sync::CancellationToken;

use crate::ledger::{PATIENCE, thief};
use crate::poster::Recorder;
use crate::{Ledger, POLL, sql, until};

/// A claimed job whose row another replica retook before its cycle began, as
/// one does after the claim lapses while the job waits, is never posted: the
/// cycle's opening count finds the row gone and ends the cycle there.
// Current-thread: the capture is per thread, and every task the lanes spawn
// runs on this one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_holder_whose_row_was_retaken_before_its_cycle_posts_nothing() -> Result<(), &'static str>
{
    let capture = Capture::install();
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "retaken while waiting").await;
    let claimed = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the claim");
    let [job] = claimed.as_slice() else {
        return Err("one row is claimed");
    };
    ledger.steal(&event).await;
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let lanes = Lanes::new(
        Posters {
            slack: poster.clone(),
        },
        ledger.database.clone(),
        token.clone(),
    );

    lanes.dispatch(Box::new(job.clone())).await;
    tokio::time::timeout(PATIENCE, lanes.finished())
        .await
        .expect("the cycle ends");
    token.cancel();
    lanes.drain().await;

    assert_eq!(poster.attempts_of(&event), 0, "the answer was not posted");
    let row = ledger.row(&event).await;
    assert_eq!(row.attempt_count, 0, "no cycle was counted");
    assert_eq!(row.receipt, thief(), "the new holder keeps it");
    let lapsed = capture.only("outbound_claim_lapsed");
    assert_eq!(lapsed.field("write"), Some("count"));
    ledger.cleanup().await;
    Ok(())
}

/// The cycle's opening count holds the claim as well as counting: a claim that
/// lapsed while nobody retook it is live again, so no replica takes the row
/// from under the post that follows.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_opening_count_extends_a_claim_nobody_retook() -> Result<(), &'static str> {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "lapsed unretaken").await;
    let claimed = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the claim");
    let [job] = claimed.as_slice() else {
        return Err("one row is claimed");
    };
    sql(
        &ledger.database,
        "UPDATE fleet.obligations SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        &[&job.id],
    )
    .await;

    let counted = obligation::count_attempt(&ledger.database, job)
        .await
        .expect("the count runs");

    assert_eq!(counted, Some(1), "the holder's token still matches");
    let retake = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("a retaking claim");
    assert!(retake.is_empty(), "the counted claim is live again");
    assert_eq!(ledger.row(&event).await.receipt, job.token);
    ledger.cleanup().await;
    Ok(())
}

/// With every delivery slot busy, a worker claims nothing more: a row it
/// claimed now would wait for a slot without renewing, and could lapse to
/// another replica. The row is left for another replica to take.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_worker_claims_only_what_its_lanes_can_start() -> Result<(), &'static str> {
    let ledger = Ledger::create().await;
    let mut held = Vec::new();
    for n in 0..IN_FLIGHT_DELIVERIES {
        let lane = ledger.lane().await;
        held.push(ledger.owe(&lane, &format!("held {n}")).await);
    }
    let spare_lane = ledger.lane().await;
    let spare = ledger.owe(&spare_lane, "beyond the slots").await;
    let gate = CancellationToken::new();
    let poster = Recorder::default().with_gate(gate.clone());
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster, POLL).await.run(token.clone()));

    until(|| held.iter().all(|event| poster.attempts_of(event) == 1)).await;
    let replica = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("another replica's claim");
    let [spare_row] = replica.as_slice() else {
        return Err("the row beyond the slots is left for another replica");
    };
    assert_eq!(spare_row.outgoing.event_id, spare);
    assert!(
        obligation::stamp_delivered(&ledger.database, spare_row)
            .await
            .expect("the other replica stamps"),
        "the other replica holds the row"
    );
    gate.cancel();
    let total = i64::try_from(IN_FLIGHT_DELIVERIES + 1).expect("a small count");
    ledger.until_delivered(total).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(poster.attempts_of(&spare), 0, "this worker never held it");
    for event in &held {
        assert_eq!(poster.attempts_of(event), 1, "{event} was posted once");
    }
    ledger.cleanup().await;
    Ok(())
}
