//! How a delivery cycle ends: refused, failed, renewed or lapsed,
//! each matched to the holder's token so a lapsed holder writes nothing.

use std::time::Duration;

use af_core::test_util::trace::Capture;
use afd_db::test_util::mint_id;
use afd_outbound::delivery;
use afd_outbound::{Lanes, Posters};
use tokio_util::sync::CancellationToken;

use crate::poster::Recorder;
use crate::{Ledger, until};

/// Every attempt a ladder makes before its cycle ends retryably.
const LADDER: usize = 3;
const _: () = assert!(LADDER == afd_outbound::retry::DELIVERY_ATTEMPTS);

/// A delivery slow enough that the test can act while it is in flight.
const IN_FLIGHT: Duration = Duration::from_millis(800);

/// A renewal fast enough to beat [`IN_FLIGHT`] several times over.
const FAST_RENEW: Duration = Duration::from_millis(50);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_refused_answer_is_abandoned_once() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "refused").await;
    let poster = Recorder::refusing(event.clone());
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster).await.run(token.clone()));

    let row = ledger.until(&event, |row| row.abandoned).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert!(!row.delivered, "an abandoned answer is not delivered");
    assert_eq!(poster.attempts_of(&event), 1, "a refusal is not retried");
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_unknown_connector_is_abandoned_unaddressable() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "nowhere").await;
    crate::sql(
        &ledger.database,
        "UPDATE thread.deliveries SET provider = 'retired' WHERE turn_id = $1",
        &[&event],
    )
    .await;
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster).await.run(token.clone()));

    ledger.until(&event, |row| row.abandoned).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(poster.attempts_of(&event), 0, "no poster is tried");
    ledger.cleanup().await;
}

/// A delivery whose every inline attempt ends retryably is marked failed, and
/// a claim that runs after it never offers it again: no retry (Indy, Nov 21,
/// 2026).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_failed_delivery_is_marked_failed_and_not_retried() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "failed").await;
    let (bus, server) = afd_bus::test_util::detached().expect("a detached bus");
    let poster = Recorder::failing(event.clone());
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker_on(bus, &poster).run(token.clone()));

    let row = ledger.until(&event, |row| row.abandoned).await;
    // A later answer and its ping make the worker claim again.
    let later_lane = ledger.lane().await;
    let later = ledger.owe(&later_lane, "later").await;
    server.publish(&afd_bus::Subject::DeliveryWake, "");
    ledger.until(&later, |row| row.delivered).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(row.attempt_count, 1, "one cycle counted");
    assert!(!row.delivered, "a failed answer is not delivered");
    assert_eq!(poster.attempts_of(&event), LADDER, "the ladder ran once");
    let after = ledger.row(&event).await;
    assert_eq!(after.attempt_count, 1, "no second cycle was counted");
    assert!(after.abandoned && !after.delivered, "it stays failed");
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_slow_delivery_renews_its_claim() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "slow").await;
    let poster = Recorder::default().with_slow(IN_FLIGHT);
    let token = CancellationToken::new();
    let worker = tokio::spawn(
        ledger
            .worker(&poster)
            .await
            .with_claim_renew(FAST_RENEW)
            .run(token.clone()),
    );

    until(|| poster.attempts_of(&event) == 1).await;
    let claimed = ledger.row(&event).await.claim_expires;
    tokio::time::sleep(FAST_RENEW * 6).await;
    let renewed = ledger.row(&event).await.claim_expires;
    ledger.until(&event, |row| row.delivered).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert!(renewed > claimed, "{renewed} must be later than {claimed}");
    ledger.cleanup().await;
}

/// Whether the capture holds a lapsed write named `write`.
fn lapsed(capture: &Capture, write: &str) -> bool {
    capture.events().iter().any(|event| {
        event.field("event") == Some("outbound_claim_lapsed") && event.field("write") == Some(write)
    })
}

// Current-thread: the capture is per thread, and every task the worker spawns
// runs on this one.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_renewal_that_finds_the_row_taken_ends_the_cycle() {
    let capture = Capture::install();
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "taken").await;
    let poster = Recorder::default().with_slow(IN_FLIGHT);
    let token = CancellationToken::new();
    let worker = tokio::spawn(
        ledger
            .worker(&poster)
            .await
            .with_claim_renew(FAST_RENEW)
            .run(token.clone()),
    );

    until(|| poster.attempts_of(&event) == 1).await;
    ledger.steal(&event).await;
    until(|| lapsed(&capture, "renew")).await;
    token.cancel();
    worker.await.expect("the worker stops");

    let row = ledger.row(&event).await;
    assert!(!row.delivered, "the cycle ended before it could stamp");
    assert_eq!(
        row.receipt,
        super::ledger::thief(),
        "the new holder keeps it"
    );
    ledger.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_lapsed_holder_neither_stamps_nor_marks_failed() {
    let capture = Capture::install();
    let ledger = Ledger::create().await;
    let (a, b) = (ledger.lane().await, ledger.lane().await);
    let delivered = ledger.owe(&a, "stamped late").await;
    let failed = ledger.owe(&b, "failed late").await;
    let poster = Recorder::failing(failed.clone()).with_slow(Duration::from_millis(300));
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster).await.run(token.clone()));

    until(|| poster.attempts_of(&delivered) == 1 && poster.attempts_of(&failed) == 1).await;
    ledger.steal(&delivered).await;
    ledger.steal(&failed).await;
    until(|| lapsed(&capture, "stamp") && poster.attempts_of(&failed) == LADDER).await;
    token.cancel();
    // Draining waits for the failed cycle's last write.
    worker.await.expect("the worker stops");

    assert!(
        !ledger.row(&delivered).await.delivered,
        "the late stamp changed nothing"
    );
    assert!(
        !ledger.row(&failed).await.abandoned,
        "the late failure mark changed nothing"
    );
    ledger.cleanup().await;
}

/// A refusal for a row another holder took while the post was in flight
/// stamps nothing and announces nothing: abandoning it is the current
/// holder's call.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refusal_for_a_row_another_holder_took_is_neither_stamped_nor_announced() {
    let capture = Capture::install();
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "taken before refused").await;
    let claimed = delivery::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the claim");
    assert_eq!(claimed.len(), 1, "one row is claimed");
    let job = claimed.into_iter().next().expect("the claimed row");
    let gate = CancellationToken::new();
    let poster = Recorder::refusing(event.clone()).with_gate(gate.clone());
    let token = CancellationToken::new();
    let lanes = Lanes::new(
        Posters {
            slack: poster.clone(),
        },
        ledger.database.clone(),
        token.clone(),
    );

    lanes.dispatch(Box::new(job)).await;
    until(|| poster.attempts_of(&event) == 1).await;
    ledger.steal(&event).await;
    gate.cancel();
    tokio::time::timeout(crate::ledger::PATIENCE, lanes.finished())
        .await
        .expect("the cycle ends");
    token.cancel();
    lanes.drain().await;

    assert_eq!(poster.attempts_of(&event), 1, "the destination refused it");
    let row = ledger.row(&event).await;
    assert!(
        !row.abandoned,
        "the holder that lost the row stamped nothing"
    );
    assert_eq!(
        row.receipt,
        super::ledger::thief(),
        "the new holder keeps it"
    );
    let said = |name: &str| {
        capture
            .events()
            .iter()
            .filter(|event| event.field("event") == Some(name))
            .count()
    };
    assert_eq!(
        said("outbound_delivery_abandoned"),
        0,
        "nothing to announce"
    );
    assert_eq!(said("outbound_delivery_abandon_failed"), 0, "the write ran");
    ledger.cleanup().await;
}
