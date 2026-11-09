//! How a delivery cycle ends: refused, retried, spent, renewed or lapsed
//! (M244 §4).

use std::time::Duration;

use af_core::test_util::trace::Capture;
use afd_outbound::lanes::MAX_DELIVERY_CYCLES;
use tokio_util::sync::CancellationToken;

use crate::poster::Recorder;
use crate::{Ledger, POLL, until};

/// Every attempt a ladder makes before its cycle ends retryably.
const LADDER: usize = 3;
const _: () = assert!(LADDER == afd_outbound::retry::DELIVERY_ATTEMPTS);

/// A delivery slow enough that the test can act while it is in flight.
const IN_FLIGHT: Duration = Duration::from_millis(800);

/// A renewal fast enough to beat [`IN_FLIGHT`] several times over.
const FAST_RENEW: Duration = Duration::from_millis(50);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refused_answer_is_abandoned_once() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "refused").await;
    let poster = Recorder::refusing(event.clone());
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster, POLL).run(token.clone()));

    let row = ledger.until(&event, |row| row.abandoned).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(row.abandon_reason.as_deref(), Some("refused"));
    assert!(!row.delivered, "an abandoned answer is not delivered");
    assert_eq!(poster.attempts_of(&event), 1, "a refusal is not retried");
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retryable_cycle_is_released_due_later() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "retried").await;
    let poster = Recorder::failing(event.clone());
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster, POLL).run(token.clone()));

    let row = ledger.until(&event, |row| row.due_later).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(row.attempt_count, 1, "one cycle counted");
    assert!(!row.delivered && !row.abandoned, "the row stays owed");
    assert_eq!(poster.attempts_of(&event), LADDER, "the ladder ran in full");
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn spent_cycles_abandon_the_answer() {
    const _: () = assert!(MAX_DELIVERY_CYCLES == 12, "the fixture seeds one short");
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "spent").await;
    crate::sql(
        &ledger.database,
        "UPDATE fleet.obligations SET attempt_count = 11 WHERE event_id = $1",
        &[&event],
    )
    .await;
    let poster = Recorder::failing(event.clone());
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster, POLL).run(token.clone()));

    let row = ledger.until(&event, |row| row.abandoned).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(row.abandon_reason.as_deref(), Some("cycles_exhausted"));
    assert_eq!(row.attempt_count, MAX_DELIVERY_CYCLES);
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_slow_delivery_renews_its_claim() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "slow").await;
    let poster = Recorder::default().with_slow(IN_FLIGHT);
    let token = CancellationToken::new();
    let worker = tokio::spawn(
        ledger
            .worker(&poster, POLL)
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
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_renewal_that_finds_the_row_taken_ends_the_cycle() {
    let capture = Capture::install();
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "taken").await;
    let poster = Recorder::default().with_slow(IN_FLIGHT);
    let token = CancellationToken::new();
    let worker = tokio::spawn(
        ledger
            .worker(&poster, POLL)
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
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_lapsed_holder_neither_stamps_nor_releases() {
    let capture = Capture::install();
    let ledger = Ledger::create().await;
    let (a, b) = (ledger.lane().await, ledger.lane().await);
    let delivered = ledger.owe(&a, "stamped late").await;
    let retried = ledger.owe(&b, "released late").await;
    let poster = Recorder::failing(retried.clone()).with_slow(Duration::from_millis(300));
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster, POLL).run(token.clone()));

    until(|| poster.attempts_of(&delivered) == 1 && poster.attempts_of(&retried) == 1).await;
    ledger.steal(&delivered).await;
    ledger.steal(&retried).await;
    until(|| lapsed(&capture, "stamp") && lapsed(&capture, "release")).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert!(
        !ledger.row(&delivered).await.delivered,
        "the late stamp changed nothing"
    );
    assert!(
        !ledger.row(&retried).await.due_later,
        "the late release changed nothing"
    );
    ledger.cleanup().await;
}
