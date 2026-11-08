//! How a worker hears of owed answers: a ping claims at once, a full batch
//! drains without waiting, and a closed bus leaves the claim pass alone
//! (M244 §4).

use std::time::Duration;

use afd_bus::Subject;
use tokio_util::sync::CancellationToken;

use crate::poster::Recorder;
use crate::{Ledger, POLL};

/// A pass long enough that only a ping, or a drain, can explain a claim.
const NEVER: Duration = Duration::from_secs(3_600);

/// Lanes enough that one claim cannot take them all.
const LANES: i64 = 12;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_ping_claims_a_full_backlog_without_a_pass() {
    let metrics = af_observability::test_util::Capture::install();
    let pings = || metrics.sum("agentsfleet_outbound_claims_total", &[("via", "ping")]);
    let ledger = Ledger::create().await;
    let (bus, server) = afd_bus::test_util::detached().expect("a detached bus");
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker_on(bus, &poster, NEVER).run(token.clone()));
    // Let the opening claim find nothing and the subscription settle.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let before = pings();

    for _ in 0..LANES {
        let lane = ledger.lane().await;
        ledger.owe(&lane, "pinged").await;
    }
    server.publish(&Subject::OutboundWake, "");
    // One ping: a full batch claims again at once, so every lane drains.
    ledger.until_delivered(LANES).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert!(pings() - before >= 1, "the ping, not a pass, claimed");
    ledger.cleanup().await;
}

// Current-thread: the capture is per thread, and the worker runs on this one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_claim_the_database_refuses_is_logged_and_retried() {
    let capture = af_core::test_util::trace::Capture::install();
    let ledger = Ledger::create().await;
    ledger.database.close().await;
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster, POLL).run(token.clone()));

    crate::until(|| {
        capture
            .events()
            .iter()
            .filter(|event| event.field("event") == Some("outbound_claim_failed"))
            .count()
            >= 2
    })
    .await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert!(poster.attempted().is_empty(), "nothing was claimed");
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_closed_bus_leaves_the_claim_pass() {
    let ledger = Ledger::create().await;
    let (bus, _server) = afd_bus::test_util::detached().expect("a detached bus");
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(
        ledger
            .worker_on(bus.clone(), &poster, Duration::from_millis(200))
            .run(token.clone()),
    );
    tokio::time::sleep(POLL * 5).await;
    bus.shutdown().await.expect("the bus closes");

    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "after the bus").await;
    ledger.until_delivered(1).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(poster.delivered_to(&lane.workspace), vec![event]);
    ledger.cleanup().await;
}
