//! The requeue pass with a live ledger and a queue that is not there.
//!
//! The sibling suite proves what a pass APPENDS. This proves what it does when
//! the scan succeeds and the append cannot: the pass stops at the first
//! refusal rather than walking the rest of the batch into the same one, and
//! every row stays owed for the next pass. A queue that refused one append
//! will refuse the next, so continuing would cost a round trip per row and
//! change nothing.
//!
//! The dead queue is a handle this test owns — port 1 is reserved and unbound,
//! so a command fails on connection refusal rather than waiting out a budget —
//! never the lane's server, which every binary running in parallel shares.
#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, naming \
              which row it was"
)]

use std::time::Duration;

use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::streams::ACKNOWLEDGED_HISTORY;
use afd_dragonfly::{Dragonfly, OutboundJob, OutboundQueue};
use afd_outbound::obligation::{self, Delivery};
use afd_outbound::producer::Producer;
use tokio_util::sync::CancellationToken;

#[path = "support/obligation_seed.rs"]
#[allow(
    dead_code,
    reason = "shared support: each suite seeds a different part of the ledger"
)]
mod seed;
#[path = "support/outbound_harness.rs"]
#[allow(
    dead_code,
    reason = "shared support: each suite drives a different part of the harness"
)]
mod support;

use seed::{FLEET, SEEDED_AT, WORKSPACE, clear_obligations, obligation_id, seed_parents};
use support::{OUTBOUND_LANE, OutboundHarness};

const PROVIDER: &str = Provider::Slack.id();

/// The thread every owed answer here is addressed to.
const DESTINATION: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;
const ANSWER: &str = "Aurora is healthy.";
const EVENT_ID: &str = "1760000000001-0";
/// The stem the trim proof numbers its answers off, so every entry it appends
/// is its own logical event rather than one event appended many times.
const EVENT_ID_STEM: &str = "1760000000002-";
/// Entries appended ABOVE the retained bound, so the trim reaches its floor
/// calculation instead of returning early on a short stream.
const ABOVE_THE_BOUND: usize = 50;
/// A cutoff every seeded row is older than, so a scan sees all of them.
const AFTER_EVERYTHING: i64 = SEEDED_AT + 1;
/// More rows than this test seeds, so a limit never decides an assertion.
const AMPLE: i64 = 64;
/// Long enough for a pass to reach the refused append, short enough that a
/// producer which never stops fails here rather than in the lane's timeout.
const PASS_BUDGET: Duration = Duration::from_secs(10);

/// A queue nobody is listening on.
fn unreachable_queue() -> OutboundQueue {
    let config =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1/".to_owned());
    OutboundQueue::new(Dragonfly::unreachable(&config).expect("a well-formed URL builds a handle"))
}

/// Owes one delivery the way the report does — in a transaction that commits.
async fn owe_committed(harness: &OutboundHarness, row: &str, answer: &str) -> bool {
    use sqlx::Acquire as _;

    let mut connection = harness
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    let mut transaction = connection
        .begin()
        .await
        .expect("the report's transaction opens");
    let written = obligation::owe(
        &mut transaction,
        row,
        Delivery {
            fleet_id: FLEET,
            workspace_id: WORKSPACE,
            provider: Provider::Slack,
            destination: DESTINATION,
            event_id: EVENT_ID,
            answer,
        },
        UnixMillis::from_millis(SEEDED_AT),
    )
    .await
    .expect("owing a delivery");
    transaction
        .commit()
        .await
        .expect("the report's transaction commits");
    written
}

/// A scan that reads rows and then cannot append leaves every one of them
/// owed, and does not end the producer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_refused_append_leaves_every_scanned_row_still_owed() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;
    seed_parents(&harness.database).await;
    clear_obligations(&harness.database).await;

    let row = obligation_id(1);
    assert!(
        owe_committed(&harness, &row, ANSWER).await,
        "the first owe creates the row"
    );

    // `unreceipted`, not `undelivered`: a row whose append never happened has
    // a NULL receipt, which is the scan the producer's first pass runs.
    let owed_before = obligation::unreceipted(
        &harness.database,
        UnixMillis::from_millis(AFTER_EVERYTHING),
        AMPLE,
    )
    .await
    .expect("the ledger answers a scan");
    assert_eq!(owed_before.len(), 1, "the seeded row is the one owed");

    let token = CancellationToken::new();
    let running = tokio::spawn(
        Producer::new(unreachable_queue(), harness.database.clone()).run(token.clone()),
    );
    // The pass reaches the refused append and reports it. The producer must
    // still be parked afterwards: one unavailable store does not take the
    // delivery path down for the life of the process.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !running.is_finished(),
        "a refused append ended the producer"
    );
    token.cancel();
    tokio::time::timeout(PASS_BUDGET, running)
        .await
        .expect("a cancelled producer stops inside the budget")
        .expect("the producer task finished cleanly");

    let owed_after = obligation::unreceipted(
        &harness.database,
        UnixMillis::from_millis(AFTER_EVERYTHING),
        AMPLE,
    )
    .await
    .expect("the ledger answers a scan");
    assert_eq!(
        owed_after.len(),
        1,
        "a row whose append was refused is still owed, so the next pass finds it"
    );
    assert_eq!(owed_after[0].id, row);

    clear_obligations(&harness.database).await;
}

/// An answer with nothing in it owes no delivery at all.
///
/// The check is before the statement: a row written for an empty answer would
/// be a delivery the producer keeps trying to make and the destination would
/// keep receiving as silence.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_empty_answer_owes_nothing() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;
    seed_parents(&harness.database).await;
    clear_obligations(&harness.database).await;

    assert!(
        !owe_committed(&harness, &obligation_id(2), "").await,
        "an empty answer is not an obligation"
    );
    let owed = obligation::unreceipted(
        &harness.database,
        UnixMillis::from_millis(AFTER_EVERYTHING),
        AMPLE,
    )
    .await
    .expect("the ledger answers a scan");
    assert!(owed.is_empty(), "an empty answer wrote a row: {owed:?}");
}

/// Trimming the outbound stream never removes an answer the group has not
/// taken, even with far more history on it than the bound retains.
///
/// The entry count matters and is the whole test. `trim_history` returns
/// early while the stream is at or under [`ACKNOWLEDGED_HISTORY`], so a stream
/// with a handful of entries never reaches the floor calculation at all — it
/// would answer "removed nothing" for a reason that has nothing to do with
/// protecting anything. Past the bound, the floor is the MINIMUM of the
/// group's last-delivered id, its oldest pending entry and the history floor,
/// and a group that has taken nothing pins that at the very start of the
/// stream.
///
/// What a regression here costs: an answer trimmed before its destination took
/// it is gone. The obligation row is repairable by the producer's own scan;
/// the stream entry is not.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_trim_keeps_the_answers_the_group_has_not_taken() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;

    let appended = ACKNOWLEDGED_HISTORY + ABOVE_THE_BOUND;
    for nth in 0..appended {
        let event_id = format!("{EVENT_ID_STEM}{nth}");
        harness
            .queue
            .enqueue(OutboundJob {
                provider: PROVIDER,
                destination: DESTINATION,
                workspace_id: WORKSPACE,
                fleet_id: FLEET,
                event_id: &event_id,
                answer: ANSWER,
            })
            .await
            .expect("the lane's queue takes an append");
    }

    assert!(
        appended > ACKNOWLEDGED_HISTORY,
        "the stream must be OVER the bound, or the trim returns before it ever \
         computes a floor and this proves nothing"
    );

    let trimmed = harness
        .queue
        .trim()
        .await
        .expect("the stream answers a trim");
    assert_eq!(
        trimmed.removed, 0,
        "an answer no consumer has taken was trimmed away: {trimmed:?}"
    );
    assert_eq!(
        trimmed.retained,
        u64::try_from(appended).expect("the appended count fits"),
        "the trim must leave every untaken answer where it is: {trimmed:?}"
    );
}
