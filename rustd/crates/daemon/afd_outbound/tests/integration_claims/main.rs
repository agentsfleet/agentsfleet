//! Outbound delivery claimed from Postgres, the ledger that is also the
//! queue, on a private database per test: a claim reads every lane, so a shared schema's other rows would
//! become this test's work.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod cycles;
mod ledger;
mod pings;
mod plan;
mod poster;

use std::time::Duration;

use af_core::id::Uuid7;
use afd_connector::Provider;
use afd_db::test_util::mint_id;
use afd_outbound::obligation::{self, Delivery};
use tokio_util::sync::CancellationToken;

use self::ledger::{DESTINATION, Lane, Ledger, POLL, sql, until};
use self::poster::Recorder;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn two_workers_keep_one_lane_in_order() {
    let ledger = Ledger::create().await;
    let (a, b) = (ledger.lane().await, ledger.lane().await);
    let mut expected_a = Vec::new();
    for n in 1..=10 {
        expected_a.push(ledger.owe(&a, &format!("a{n}")).await);
        ledger.owe(&b, &format!("b{n}")).await;
    }
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let first = tokio::spawn(ledger.worker(&poster, POLL).await.run(token.clone()));
    let second = tokio::spawn(ledger.worker(&poster, POLL).await.run(token.clone()));

    ledger.until_delivered(20).await;
    token.cancel();
    first.await.expect("the first worker stops");
    second.await.expect("the second worker stops");

    assert_eq!(
        poster.delivered_to(&a.workspace),
        expected_a,
        "lane A in order"
    );
    assert_eq!(
        poster.most_in_flight(&a.workspace),
        1,
        "never two rows held in lane A"
    );
    assert_eq!(
        poster.most_in_flight(&b.workspace),
        1,
        "never two rows held in lane B"
    );
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn failing_head_blocks_only_its_lane() {
    let ledger = Ledger::create().await;
    let (a, b) = (ledger.lane().await, ledger.lane().await);
    let head = ledger.owe(&a, "a1").await;
    let behind = ledger.owe(&a, "a2").await;
    for n in 1..=5 {
        ledger.owe(&b, &format!("b{n}")).await;
    }
    let poster = Recorder::failing(head.clone());
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster, POLL).await.run(token.clone()));

    ledger.until_delivered(5).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(poster.delivered_to(&b.workspace).len(), 5, "lane B drained");
    assert!(
        !poster.attempted().contains(&behind),
        "A's second row is never claimed before its head"
    );
    assert!(poster.attempted().contains(&head), "A's head was tried");
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn lapsed_claim_is_retaken_and_the_late_stamp_refused() -> Result<(), &'static str> {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    ledger.owe(&lane, "only").await;

    let first = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the first claim");
    let [first] = first.as_slice() else {
        return Err("one row is claimed");
    };
    sql(
        &ledger.database,
        "UPDATE fleet.obligations SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        &[&first.id],
    )
    .await;
    let second = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the retake");
    let [second] = second.as_slice() else {
        return Err("a lapsed claim is retaken");
    };
    assert_eq!(second.id, first.id);

    let stale_stamp = obligation::stamp_delivered(&ledger.database, first)
        .await
        .expect("the late stamp runs");
    assert!(!stale_stamp, "the first holder's late stamp updates 0 rows");
    let current = obligation::stamp_delivered(&ledger.database, second)
        .await
        .expect("the holder's stamp runs");
    assert!(current, "the current holder stamps");
    assert_eq!(ledger.delivered().await, 1, "one delivery in total");
    ledger.cleanup().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn lost_ping_is_caught_by_the_claim_pass() {
    let metrics = af_observability::test_util::Capture::install();
    let scans = || metrics.sum("agentsfleet_outbound_claims_total", &[("via", "scan")]);
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(
        ledger
            .worker(&poster, Duration::from_millis(200))
            .await
            .run(token.clone()),
    );
    // Let the worker's opening claim find nothing before the row exists.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let before = scans();

    let event = ledger.owe(&lane, "unpinged").await;
    ledger.until_delivered(1).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(poster.delivered_to(&lane.workspace), vec![event]);
    // At least one: the capture is process-wide and sibling tests also scan,
    // but none of them pings, so any rise here is a pass.
    assert!(
        scans() - before >= 1,
        "the pass, not a ping, claimed the row"
    );
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_empty_answer_owes_nothing_and_a_repeat_owes_once() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = mint_id();
    let mut connection = ledger.database.acquire().await.expect("a connection");
    let delivery = |answer| Delivery {
        fleet_id: &lane.fleet,
        workspace_id: &lane.workspace,
        provider: Provider::Slack,
        destination: DESTINATION,
        event_id: &event,
        answer,
    };
    assert!(
        !obligation::owe(&mut connection, &mint_id(), &mint_id(), delivery(""))
            .await
            .expect("owe")
    );
    assert!(
        obligation::owe(&mut connection, &mint_id(), &mint_id(), delivery("x"))
            .await
            .expect("owe")
    );
    assert!(
        !obligation::owe(&mut connection, &mint_id(), &mint_id(), delivery("x"))
            .await
            .expect("owe")
    );
    drop(connection);
    ledger.cleanup().await;
}

/// A row that commits after a later row of its lane was claimed waits behind
/// that claim: `sequence` is drawn at insert, not at commit, so the lower one
/// is not the lane's head while the higher one is held.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_late_commit_waits_behind_a_held_claim() -> Result<(), &'static str> {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let (early, early_event) = owe_uncommitted(&ledger, &lane, "early").await;
    let late_event = ledger.owe(&lane, "late").await;

    let first = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the first claim");
    let [holding] = first.as_slice() else {
        return Err("only the committed row is claimed");
    };
    assert_eq!(holding.outgoing.event_id, late_event);
    early.commit().await.expect("the earlier report commits");

    let while_held = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the second claim");
    assert!(while_held.is_empty(), "a held lane offers nothing more");
    assert!(
        obligation::stamp_delivered(&ledger.database, holding)
            .await
            .expect("the holder stamps")
    );
    let after = obligation::claim(&ledger.database, 8, &mint_id())
        .await
        .expect("the claim after delivery");
    let [next] = after.as_slice() else {
        return Err("the earlier row is offered once the lane is free");
    };
    assert_eq!(next.outgoing.event_id, early_event);
    ledger.cleanup().await;
    Ok(())
}

/// Owes `answer` on `lane` inside a report transaction left open, so its row
/// draws its `sequence` now and commits when the caller says.
async fn owe_uncommitted(
    ledger: &Ledger,
    lane: &Lane,
    answer: &str,
) -> (sqlx::Transaction<'static, sqlx::Postgres>, Uuid7) {
    let connection = ledger.database.acquire().await.expect("a connection");
    let mut report = sqlx::Transaction::begin(connection, None)
        .await
        .expect("the report's transaction");
    let event = mint_id();
    obligation::owe(
        &mut report,
        &mint_id(),
        &mint_id(),
        Delivery {
            fleet_id: &lane.fleet,
            workspace_id: &lane.workspace,
            provider: Provider::Slack,
            destination: DESTINATION,
            event_id: &event,
            answer,
        },
    )
    .await
    .expect("the report owes");
    (report, event)
}

/// Claimers released together never hold two rows of one lane.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn claimers_released_together_hold_one_row_per_lane() {
    const CLAIMERS: usize = 100;
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    for number in 0..10 {
        ledger.owe(&lane, &format!("answer {number}")).await;
    }
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(CLAIMERS));
    let mut claims = tokio::task::JoinSet::new();
    for _ in 0..CLAIMERS {
        let (database, barrier) = (ledger.database.clone(), std::sync::Arc::clone(&barrier));
        claims.spawn(async move {
            barrier.wait().await;
            obligation::claim(&database, 8, &mint_id())
                .await
                .expect("a claim")
                .len()
        });
    }
    let held: usize = claims.join_all().await.into_iter().sum();
    assert_eq!(
        held, 1,
        "one lane, one held row, however many claim at once"
    );
    ledger.cleanup().await;
}
