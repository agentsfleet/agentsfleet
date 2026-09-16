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

use afd_core::clock::UnixMillis;
use afd_dragonfly::{OutboundJob, Redis};
use std::time::Duration;

use afd_outbound::obligation::{self, Delivery};
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
    FLEET, SEEDED_AT, WORKSPACE, clear_obligations, entries_naming, entries_on, forget_group,
    forget_stream, obligation_id, reader_named, seed_parents,
};
use support::{OUTBOUND_LANE, OutboundHarness};

/// The connector every fixture answer goes back through.
const PROVIDER: &str = "slack";
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

/// One owed answer, addressed.
fn delivery(event_id: &str) -> Delivery<'_> {
    Delivery {
        fleet_id: FLEET,
        workspace_id: WORKSPACE,
        provider: PROVIDER,
        event_id,
        answer: ANSWER,
    }
}

/// Owes one delivery the way the report does — in a transaction that commits.
///
/// Returns whether this call is the one that created the row, which is what
/// `owe` answers and what the caller uses to decide whether to append.
async fn owe_committed(harness: &OutboundHarness, row: &str, event_id: &str) -> bool {
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
        delivery(event_id),
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

/// A fixture in the state each test starts from: parents seeded, nothing owed.
async fn ready() -> OutboundHarness {
    let harness = OutboundHarness::reset().await;
    seed_parents(&harness.database).await;
    clear_obligations(&harness.database).await;
    harness
}

/// The event ids one scan answers, oldest first.
///
/// Both scans take the same three arguments and differ only in WHICH set they
/// name, so they share a body here for the same reason `obligation::write_receipt`
/// shares one between its two statements: a pair that binds the same parameters
/// in the same order is the pair that drifts when each keeps its own copy.
/// One of the producer's two recovery scans, as a value.
///
/// Named because the signature is unreadable inline and clippy says so: both
/// scans are `async fn`s with identical shapes, and the only way to pass either
/// to one body is as a function pointer returning a boxed future.
type Scan = for<'a> fn(
    &'a afd_db::Db,
    UnixMillis,
    i64,
) -> std::pin::Pin<
    Box<dyn Future<Output = afd_outbound::Result<Vec<obligation::Owed>>> + Send + 'a>,
>;

async fn owing(harness: &OutboundHarness, scan: Scan) -> Vec<String> {
    scan(
        &harness.database,
        UnixMillis::from_millis(AFTER_EVERYTHING),
        AMPLE,
    )
    .await
    .expect("the scan answers")
    .into_iter()
    .map(|owed| owed.event_id)
    .collect()
}

/// Obligations still owed an APPEND — committed, never queued.
async fn awaiting_append(harness: &OutboundHarness) -> Vec<String> {
    owing(harness, |db, before, limit| {
        Box::pin(obligation::unreceipted(db, before, limit))
    })
    .await
}

/// Obligations still owed a DELIVERY — queued, nobody received them.
async fn awaiting_delivery(harness: &OutboundHarness) -> Vec<String> {
    owing(harness, |db, before, limit| {
        Box::pin(obligation::undelivered(db, before, limit))
    })
    .await
}

/// Owes an answer AND puts it on the queue, the way the report's fast path does.
///
/// Three steps in the production order: the transaction commits the obligation,
/// the append follows it, and the receipt records which entry carries it. The
/// tests below each remove one of those and assert what is left.
async fn owe_and_queue(harness: &OutboundHarness, nth: u8, event: &str) {
    assert!(
        owe_committed(harness, &obligation_id(nth), event).await,
        "each fixture answer owes its own delivery"
    );
    let entry = harness
        .queue
        .enqueue(OutboundJob {
            provider: PROVIDER,
            workspace_id: WORKSPACE,
            fleet_id: FLEET,
            event_id: event,
            answer: ANSWER,
        })
        .await
        .expect("the queue takes the entry");
    obligation::receipt(
        &harness.database,
        &obligation_id(nth),
        entry.as_str(),
        UnixMillis::from_millis(SEEDED_AT),
    )
    .await
    .expect("recording the receipt");
}

/// A second handle on the lane's datastore, for the faults the queue cannot stage.
async fn datastore() -> Redis {
    Redis::connect(&OutboundHarness::config())
        .await
        .expect("the lane's Redis must be reachable")
}

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
/// FIRST took the answer, nor count a second attempt against it.
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
    assert_eq!(attempts, 1, "one acceptance is one attempt");
}

/// A worker replaced under a different hostname leaves its answer recoverable.
///
/// The dimension's named proof. A consumer name is host-derived and constant for
/// a process, so the replacement reads a pending list that is EMPTY — the dead
/// host's entries are not offered to it, and `read_blocking` never re-offers an
/// entry already handed out. The entry is therefore held by a name that will
/// never come back, and unreachable by anyone else.
///
/// Staged with two explicitly-named readers because one test process has one
/// hostname: the name is the only thing that differs from what production builds.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_outbound_obligations_survive_worker_replacement() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-3";
    owe_and_queue(&harness, 5, event).await;

    // The host that dies, taking the entry into its own pending list.
    let mut departed = reader_named(&OutboundHarness::config(), "host-that-died").await;
    let taken = departed
        .read_blocking(BLOCK_MS)
        .await
        .expect("the group offers the entry")
        .expect("an entry was queued for it");
    assert_eq!(taken.event_id, event, "the host took this fixture's answer");
    drop(departed);

    assert_eq!(
        harness.pending_count().await,
        1,
        "the dead host still holds it — an entry delivered and never acknowledged"
    );

    // Its replacement, under a new hostname.
    let mut replacement = reader_named(&OutboundHarness::config(), "host-that-replaced-it").await;
    assert!(
        replacement
            .read_pending()
            .await
            .expect("the pending read answers")
            .is_none(),
        "a new hostname inherits NOTHING: the pending list it reads is its own, \
         and the dead host's is not offered to it"
    );
    assert!(
        replacement
            .read_blocking(BLOCK_MS)
            .await
            .expect("the blocking read answers")
            .is_none(),
        "and the entry is never re-offered as new, so no worker can reach it"
    );

    assert_eq!(
        awaiting_delivery(&harness).await,
        vec![event.to_owned()],
        "the queue cannot deliver it and the ledger still owes it — which is \
         the whole claim: the obligation outlives the entry"
    );

    // Recovery is the producer re-appending. Not free — the destination may see
    // the answer twice — which is why `delivered_at` is stamped by the poster
    // and not by the acknowledgement: a row leaves this set only when somebody
    // actually got it.
    let again = harness
        .queue
        .enqueue(OutboundJob {
            provider: PROVIDER,
            workspace_id: WORKSPACE,
            fleet_id: FLEET,
            event_id: event,
            answer: ANSWER,
        })
        .await
        .expect("the re-append is accepted");
    obligation::record_reappended(
        &harness.database,
        &obligation_id(5),
        again.as_str(),
        UnixMillis::from_millis(SEEDED_AT),
    )
    .await
    .expect("recording the re-append");
    obligation::stamp_delivered(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(SEEDED_AT + 30),
    )
    .await
    .expect("the re-appended answer is delivered");

    assert!(
        awaiting_delivery(&harness).await.is_empty(),
        "re-appended and delivered: the answer survived its worker"
    );
    assert!(
        entries_on(&redis).await >= 1,
        "the re-append put a real entry on the real stream"
    );
}

/// A lost consumer group leaves the answer owed.
///
/// Its own test rather than a branch of the one above, because the fault has to
/// be applied to the whole stream: a group cannot be destroyed for one entry.
/// The entries SURVIVE here and become unreachable, which is a different shape
/// from losing the stream — and the point is that the ledger does not care.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_lost_consumer_group_leaves_the_answer_owed() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-4";
    owe_and_queue(&harness, 6, event).await;

    forget_group(&redis).await;

    assert_eq!(
        entries_on(&redis).await,
        1,
        "the entry is still there — it is the way to reach it that is gone"
    );
    assert_eq!(
        awaiting_delivery(&harness).await,
        vec![event.to_owned()],
        "queued and unreachable reads, to the ledger, as still owed"
    );
}

/// A wholly lost stream leaves the answer owed.
///
/// The harshest of the three and the one that proves the ledger is the forge:
/// entries, group and pending lists all gone at once, and the answer is still
/// owed because PostgreSQL never stopped knowing about it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_wholly_lost_stream_leaves_the_answer_owed() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-5";
    owe_and_queue(&harness, 7, event).await;

    forget_stream(&redis).await;

    assert_eq!(entries_on(&redis).await, 0, "nothing of the queue survives");
    assert_eq!(
        awaiting_delivery(&harness).await,
        vec![event.to_owned()],
        "losing the cache entirely erases no obligation"
    );
}

/// Dimension 7.6 — the DAEMON's producer appends it, and this test does not.
///
/// Every other case in this file calls a scan and then enqueues by hand. That
/// grades `obligation::unreceipted` and the queue, and it is exactly why "a
/// daemon producer enqueues it" had no proof. `Producer::run` is the loop
/// `agentsfleetd` spawns (`agentsfleetd/src/outbound.rs:95`); it computes its
/// OWN cutoffs from `clock::now()` rather than taking them as parameters, it
/// appends every row a scan answers, and it records each entry as that row's
/// receipt. None of that is reached by calling a scan directly, and none of it
/// had a test: nothing under `tests/` named `afd_outbound::producer`, and
/// `producer.rs` carries no `#[cfg(test)]` module.
///
/// Dimensions 4.2 and 4.3 are closed by THIS Dimension on the finding that
/// nothing in the daemon called `OutboundQueue::enqueue`. This is the test that
/// fails if that becomes true again.
///
/// No clock is waited on. `SEEDED_AT` is older than `producer::MIN_AGE` by
/// years, so the producer's first pass already finds the row.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn the_daemon_producer_appends_an_owed_answer_and_receipts_it() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-6";

    assert!(owe_committed(&harness, &obligation_id(8), event).await);
    assert_eq!(entries_on(&redis).await, 0, "nothing has appended it yet");

    let token = CancellationToken::new();
    let running = tokio::spawn(
        Producer::new(harness.queue.clone(), harness.database.clone()).run(token.clone()),
    );

    let appended = timeout(PRODUCER_PASS, async {
        while entries_on(&redis).await == 0 {
            sleep(POLL).await;
        }
    })
    .await;

    // Cancelled before any assertion, so a failing claim still stops the loop
    // rather than leaving it appending under the next test's lane lock.
    token.cancel();
    running
        .await
        .expect("the producer stops when its token is cancelled");

    assert!(
        appended.is_ok(),
        "the producer's own pass appended nothing: the daemon path that closes \
         Dimensions 4.2 and 4.3 never reached `OutboundQueue::enqueue`"
    );
    assert_eq!(
        entries_naming(&redis, event).await,
        1,
        "the owed answer was appended exactly once — asked about THIS event \
         rather than about the stream, which the producer also fills with every \
         other fleet's owed answers, correctly"
    );
    assert!(
        awaiting_append(&harness).await.is_empty(),
        "the producer receipted the row it appended, so the next pass does not \
         append the same answer a second time"
    );
}
