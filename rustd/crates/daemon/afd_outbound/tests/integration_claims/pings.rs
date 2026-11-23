//! How a worker hears of owed answers: a ping claims at once, and a full batch
//! drains without waiting. The worker runs no timed pass.

use std::time::Duration;

use afd_bus::Subject;
use tokio_util::sync::CancellationToken;

use crate::Ledger;
use crate::poster::Recorder;

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
    let worker = tokio::spawn(ledger.worker_on(bus, &poster).run(token.clone()));
    // Let the opening claim find nothing and the subscription settle.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let before = pings();

    for _ in 0..LANES {
        let lane = ledger.lane().await;
        ledger.owe(&lane, "pinged").await;
    }
    server.publish(&Subject::DeliveryWake, "");
    // One ping: a full batch claims again at once, so every lane drains.
    ledger.until_delivered(LANES).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert!(pings() - before >= 1, "the ping claimed");
    ledger.cleanup().await;
}

// Current-thread: the capture is per thread, and the worker runs on this one.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_claim_the_database_refuses_is_logged() {
    let capture = af_core::test_util::trace::Capture::install();
    let ledger = Ledger::create().await;
    ledger.database.close().await;
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster).await.run(token.clone()));

    crate::until(|| {
        capture
            .events()
            .iter()
            .filter(|event| event.field("event") == Some("outbound_claim_failed"))
            .count()
            >= 1
    })
    .await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert!(poster.attempted().is_empty(), "nothing was claimed");
    let unavailable = af_core::error_code::INTERNAL_DB_UNAVAILABLE.as_str();
    let failed: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("outbound_claim_failed"))
        .collect();
    assert!(
        failed
            .iter()
            .all(|event| event.field("error_code") == Some(unavailable)),
        "a closed pool is the outage an operator retries against: {failed:?}"
    );
    ledger.cleanup().await;
}

// Current-thread: the capture is per thread, and the worker runs on this one.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_claim_that_takes_a_row_logs_how_many_it_took() {
    let capture = af_core::test_util::trace::Capture::install();
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    ledger.owe(&lane, "claimed").await;
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster).await.run(token.clone()));

    ledger.until_delivered(1).await;
    token.cancel();
    worker.await.expect("the worker stops");

    let took_one = capture.events().iter().any(|event| {
        event.field("event") == Some("outbound_claim_completed")
            && event.field("claimed") == Some("1")
    });
    assert!(took_one, "the claim that took the row logs its count");
    ledger.cleanup().await;
}

/// A claim held by a replica that stopped renewing it is retaken when it lapses,
/// with no ping and no timed pass: the worker reads when the earliest live claim
/// lapses and sleeps to that instant once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_dead_replicas_claim_is_retaken_when_it_lapses() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "held by a dead replica").await;
    // Another replica claimed it, then died: its claim lapses in two seconds.
    crate::ledger::sql(
        &ledger.database,
        "UPDATE thread.deliveries SET receipt = $2, \
         claim_expires_at = clock_timestamp() + interval '2 seconds' WHERE turn_id = $1",
        &[&event, &crate::ledger::thief()],
    )
    .await;
    let (bus, _server) = afd_bus::test_util::detached().expect("a detached bus");
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker_on(bus, &poster).run(token.clone()));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(ledger.delivered().await, 0, "a live claim is not taken");
    ledger.until_delivered(1).await;
    token.cancel();
    worker.await.expect("the worker stops");
    ledger.cleanup().await;
}

/// The lapse a worker sleeps to is another holder's: a delivery this replica holds is
/// renewed by its own lane, so it never sets the worker's wake.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_lapse_leaves_out_the_deliveries_this_replica_holds() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = ledger.owe(&lane, "held here").await;
    crate::ledger::sql(
        &ledger.database,
        "UPDATE thread.deliveries SET receipt = $2, \
         claim_expires_at = clock_timestamp() + interval '30 seconds' WHERE turn_id = $1",
        &[&event, &crate::ledger::thief()],
    )
    .await;
    let id: af_core::id::Uuid7 =
        sqlx::query_scalar("SELECT id FROM thread.deliveries WHERE turn_id = $1")
            .bind(&event)
            .fetch_one(&mut *ledger.database.acquire().await.expect("a connection"))
            .await
            .expect("the delivery row");
    let anyone = afd_outbound::delivery::next_lapse(&ledger.database, &[])
        .await
        .expect("the lapse reads");
    assert!(
        anyone.is_some_and(|left| left <= Duration::from_secs(30)),
        "another holder's claim sets the lapse: {anyone:?}"
    );
    let own = afd_outbound::delivery::next_lapse(&ledger.database, &[id])
        .await
        .expect("the lapse reads");
    assert_eq!(own, None, "this replica's own claim sets no lapse");
    ledger.cleanup().await;
}
