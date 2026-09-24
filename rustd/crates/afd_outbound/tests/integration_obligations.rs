//! Dimension 7.6 — a committed result and an owed delivery are one instant.
//!
//! The report commits four things in one PostgreSQL transaction and the queue
//! append is deliberately NOT one of them, because nothing spans PostgreSQL and
//! the datastore. So the obligation row IS the delivery until an entry exists,
//! and every failure below is a variation on one question: with the entry gone,
//! missing, or never written, is the answer still owed and still recoverable?
//!
//! # Why these run against the ledger and not through `Worker::run`
//!
//! `integration_worker.rs` drives the worker loop and grades what it does with
//! an entry it was handed. That is Dimension 5.1 and 5.2 and it is covered. The
//! claims here are about what survives when the entry is NOT handed over —
//! states the worker never sees, by construction. The scans are the production
//! recovery path (`producer::run` calls exactly these), so calling them
//! directly grades the same code the daemon runs, one pass at a time.
//!
//! # No clock is waited on
//!
//! `producer::MIN_AGE` (30s) and `LOST_AFTER` (300s) keep the live report from
//! racing its own recovery. Both scans take their cutoff as a PARAMETER, so a
//! test passes one relative to [`SEEDED_AT`] and proves the same predicate
//! without sleeping for five minutes.
//!
//! # Serialised on the shared stream, like its neighbour
//!
//! `OUTBOUND_STREAM_KEY` is a constant shared with the Zig daemon and cannot be
//! namespaced per test — see `support/outbound_harness.rs` for the full reason.
//! Every test here takes [`OUTBOUND_LANE`] for the same reason that file does.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_dragonfly::{Dragonfly, OutboundJob};
use std::time::Duration;

use afd_outbound::obligation::{self, AbandonReason, Delivery};
use afd_outbound::producer::Producer;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

#[path = "support/obligation_seed.rs"]
mod seed;
#[path = "support/outbound_harness.rs"]
#[expect(
    dead_code,
    reason = "one harness, two test binaries: `integration_worker.rs` drives the \
              poisoned-entry and wrong-typed-key seams, this one drives the queue \
              and the ledger. Neither uses all of it, and splitting the file would \
              put the shared `OUTBOUND_LANE` mutex in two places — which is the one \
              thing that must stay single, since it is what serialises them."
)]
mod support;

use seed::{
    FLEET, SEEDED_AT, WORKSPACE, clear_obligations, destinations_naming, entries_naming,
    entries_on, forget_group, forget_stream, obligation_id, reader_named, seed_parents,
};
use support::{OUTBOUND_LANE, OutboundHarness};

/// The connector every fixture answer goes back through.
const PROVIDER: &str = Provider::Slack.id();

/// The thread every owed answer here is addressed to.
const DESTINATION: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;
/// What the fixture answers say.
const ANSWER: &str = "Aurora is healthy.";
/// A cutoff every seeded row is older than, so a scan sees all of them.
const AFTER_EVERYTHING: i64 = SEEDED_AT + 1;
/// More rows than any test seeds, so a scan's limit never decides an assertion.
const AMPLE: i64 = 64;
/// Long enough that a live server answers, short enough that a missing entry
/// fails this test rather than the lane's whole timeout.
const BLOCK_MS: usize = 200;
/// Long enough for the daemon producer's first pass to reach a live datastore,
/// short enough that a producer appending NOTHING fails here rather than in the
/// lane's own timeout.
const PRODUCER_PASS: Duration = Duration::from_secs(10);
/// How often that pass is looked for while it runs.
const POLL: Duration = Duration::from_millis(25);

/// A result that committed and never reached the queue is still owed.
///
/// The gap 7.6 exists to close. The transaction commits the result, the settle
/// and the obligation together; the append runs AFTER, because nothing spans
/// PostgreSQL and the datastore. A process that dies in between has charged the
/// tenant and queued nothing — and the assertion that matters is the pair: the
/// stream is empty AND the ledger still owes, which is what makes the loss
/// recoverable instead of silent.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_result_committed_without_its_append_is_still_owed() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-0";

    assert!(owe_committed(&harness, &obligation_id(1), event).await);

    assert_eq!(entries_on(&redis).await, 0, "the append never ran");
    assert_eq!(
        awaiting_append(&harness).await,
        vec![event.to_owned()],
        "the committed result is exactly what the producer's unreceipted scan \
         exists to find"
    );

    // The append the dead process never made, made by the producer's pass.
    let entry = harness
        .queue
        .enqueue(OutboundJob {
            provider: PROVIDER,
            destination: DESTINATION,
            workspace_id: WORKSPACE,
            fleet_id: FLEET,
            event_id: event,
            answer: ANSWER,
        })
        .await
        .expect("the queue takes the entry");
    obligation::receipt(
        &harness.database,
        &obligation_id(1),
        entry.as_str(),
        UnixMillis::from_millis(SEEDED_AT),
    )
    .await
    .expect("recording the receipt");

    assert_eq!(entries_on(&redis).await, 1, "recovered onto the queue");
    assert!(
        awaiting_append(&harness).await.is_empty(),
        "a receipted row is queued and no longer the producer's work"
    );
}

/// Two replicas reporting one answer owe one delivery and queue one entry.
///
/// `uq_fleet_obligations_event` makes the insert idempotent, but the claim that
/// matters is downstream of it: the loser is TOLD it wrote nothing, and that is
/// the signal it uses not to append. Asserting only the row count would pass
/// even if both replicas queued the answer twice.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn two_replicas_reporting_one_answer_owe_one_delivery() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-1";

    let first = owe_committed(&harness, &obligation_id(2), event).await;
    let second = owe_committed(&harness, &obligation_id(3), event).await;
    assert!(first, "the first replica creates the row");
    assert!(!second, "the second is told it wrote nothing");

    // Each replica appends only what it wrote — the behaviour under test.
    for wrote in [first, second] {
        if wrote {
            harness
                .queue
                .enqueue(OutboundJob {
                    provider: PROVIDER,
                    destination: DESTINATION,
                    workspace_id: WORKSPACE,
                    fleet_id: FLEET,
                    event_id: event,
                    answer: ANSWER,
                })
                .await
                .expect("the queue takes the entry");
        }
    }

    assert_eq!(
        entries_on(&redis).await,
        1,
        "one answer reaches the destination's thread once, whatever two \
         replicas both tried to report"
    );
}

/// An accepted answer is stamped once, however often the acknowledgement is lost.
///
/// The at-least-once edge: the poster delivered, the response never arrived, the
/// caller retried and delivered again. `STAMP_DELIVERED` is guarded on
/// `delivered_at IS NULL` so the retry cannot move the instant the destination
/// FIRST took the answer.
///
/// The stamp does not touch `attempt_count` at all, which is why this test now
/// asserts the counter is UNMOVED by two stampings rather than that it reads
/// one. Counting here counted successes: the branch that reaches this statement
/// is the delivered branch, so a destination that refused an answer nine times
/// and took it on the tenth recorded one attempt, and one that never took it
/// recorded none. The cycle-start counter is what records attempts now, and
/// `integration_attempt_count.rs` grades it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_destination_that_accepted_an_answer_is_stamped_once() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000000-2";
    owe_and_queue(&harness, 4, event).await;

    let accepted_at = SEEDED_AT + 10;
    let retried_at = SEEDED_AT + 20;
    for at in [accepted_at, retried_at] {
        obligation::stamp_delivered(&harness.database, FLEET, event, UnixMillis::from_millis(at))
            .await
            .expect("stamping the delivery");
    }

    assert!(
        awaiting_delivery(&harness).await.is_empty(),
        "a stamped answer owes nothing further"
    );

    let mut connection = harness
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    let (delivered_at, attempts): (i64, i64) = sqlx::query_as(
        "SELECT delivered_at, attempt_count FROM core.fleet_obligations
          WHERE fleet_id = $1::uuid AND event_id = $2::text",
    )
    .bind(FLEET)
    .bind(event)
    .fetch_one(&mut *connection)
    .await
    .expect("reading the stamped row");

    assert_eq!(
        delivered_at, accepted_at,
        "the instant the destination first took it, not the retry's"
    );
    assert_eq!(
        attempts, 0,
        "stamping a delivery counts no attempt — the cycle start does"
    );
}

#[path = "integration_obligations/destination.rs"]
mod destination;
#[path = "integration_obligations/loss.rs"]
mod loss;
#[path = "integration_obligations/owed.rs"]
mod owed;

use self::owed::*;
