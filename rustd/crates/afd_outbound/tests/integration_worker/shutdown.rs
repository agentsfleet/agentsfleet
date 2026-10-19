//! A worker told to stop loses no answer: what it had taken is delivered and
//! acknowledged, or left for the next worker to take.
//!
//! Split from the worker cases at the file cap.

use super::*;

use afd_outbound::LANE_DEPTH;

use super::gated_poster::Gated;

/// Dimension 5.2 — a shutdown mid-delivery loses nothing and duplicates
/// nothing.
///
/// # The two halves are one test because the second inherits the first's state
///
/// Phase one stops a worker during an attempt that fails, and asserts the entry
/// is left UNACKNOWLEDGED under this host's consumer name. Phase two starts a
/// second worker under that same name and asserts its pending-first read finds
/// the entry, delivers it exactly once, and acknowledges it.
///
/// Separating them would let phase two enqueue its own entry and read it with
/// `>`, which is the path that already works — the whole point is the entry
/// nothing re-offers, that only a pending read reaches.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_outbound_shutdown_no_loss() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;

    let answer = "Aurora is healthy.";
    enqueue(&harness, answer).await;

    // ── Phase one: stopped during an attempt that fails ──
    let token = CancellationToken::new();
    let interrupted_poster = Scripted::cancelling(&[Verdict::Retryable], 0, token.clone());
    let interrupted = Posters {
        slack: interrupted_poster.clone(),
    };

    let worker = Worker::new(
        harness.reader().await,
        harness.queue.clone(),
        harness.database.clone(),
        interrupted,
    );
    tokio::time::timeout(PROGRESS_BUDGET, worker.run(token.clone()))
        .await
        .expect("a cancelled worker joins inside the supervisor's budget");

    assert_eq!(
        interrupted_poster.attempts(),
        1,
        "a cancelled token stops the RETRY from starting another attempt, so a \
         shutdown costs one vendor deadline rather than the whole budget"
    );
    assert_eq!(
        harness.pending_count().await,
        1,
        "the answer is not acknowledged, which is what re-queues it: an ack \
         here would be the lost-answer failure this dimension exists to catch"
    );
    assert_eq!(
        harness.pending_consumers().await,
        vec![afd_dragonfly::outbound_consumer()],
        "the entry has to be pending under the name the NEXT process comes \
         back to; under any other it is neither delivered nor lost, just \
         permanently invisible"
    );

    // ── Phase two: the next process finds it pending-first ──
    let resumed_token = CancellationToken::new();
    let resumed_poster = Scripted::new(&[Verdict::Delivered]);
    let resumed = Posters {
        slack: resumed_poster.clone(),
    };

    let worker = Worker::new(
        harness.reader().await,
        harness.queue.clone(),
        harness.database.clone(),
        resumed,
    );
    let running = tokio::spawn(worker.run(resumed_token.clone()));

    await_until("the re-queued answer to be acknowledged", async || {
        harness.pending_count().await == 0
    })
    .await;

    resumed_token.cancel();
    running.await.expect("the worker task must not panic");

    assert_eq!(
        resumed_poster.attempts(),
        1,
        "delivered exactly once by the second process — a second attempt here \
         would be the double-delivery half of this dimension"
    );
    assert_eq!(
        resumed_poster.seen(),
        vec![answer.to_owned()],
        "and it is the answer the first process was handed, not a fresh read"
    );
}

/// A shutdown that arrives during an attempt which SUCCEEDS still acknowledges.
///
/// The other side of Dimension 5.2's "delivered once or re-queued". The
/// requeue branch in `deliver_and_ack` reads the token, so a successful
/// delivery must fall past it to the ack — a version that returned early on any
/// cancelled token would leave a DELIVERED answer pending, and the next process
/// would post it to the destination's thread a second time.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_a_shutdown_during_a_successful_delivery_still_acknowledges() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;

    enqueue(
        &harness,
        "the answer landed as the process was told to stop",
    )
    .await;

    let token = CancellationToken::new();
    let poster = Scripted::cancelling(&[Verdict::Delivered], 0, token.clone());
    let posters = Posters {
        slack: poster.clone(),
    };

    let worker = Worker::new(
        harness.reader().await,
        harness.queue.clone(),
        harness.database.clone(),
        posters,
    );
    tokio::time::timeout(PROGRESS_BUDGET, worker.run(token.clone()))
        .await
        .expect("a cancelled worker joins inside the supervisor's budget");

    assert_eq!(poster.attempts(), 1, "one attempt, and it succeeded");
    assert_eq!(
        harness.pending_count().await,
        0,
        "a delivered answer is acknowledged even though the token was cancelled \
         during it; leaving it pending would post it twice"
    );
}

/// How long the test waits, once the reader has taken all it will, for that
/// reader to reach the hand-off it then waits at.
///
/// The read's reply lands in microseconds; the margin is for a loaded host.
/// What it buys is that the shutdown below arrives at the HAND-OFF rather than
/// at the read, and the pending count asserted after it proves the reader
/// did not move on in the meantime.
const HAND_OFF_SETTLE: Duration = Duration::from_millis(200);

/// A shutdown that arrives while a full lane holds the reader stops it at
/// once: the hand-off is raced against the token, so the worker never waits
/// for a destination that will not answer to make room.
///
/// The lane takes one job in hand and queues [`LANE_DEPTH`] more, so the read
/// after those is the one whose hand-off waits, and one entry past it is
/// never read at all. The job in hand finishes and is acknowledged; every
/// other job the reader took stays pending under this host's consumer name,
/// which is the hand-over to the next process.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_a_shutdown_while_a_full_lane_holds_the_reader_stops_the_worker() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;

    let read = 1 + LANE_DEPTH + 1;
    for queued in 0..=read {
        enqueue(&harness, &format!("answer {queued}")).await;
    }

    let poster = Gated::default();
    let gate = poster.shut(WORKSPACE_ID);
    let token = CancellationToken::new();
    let running = held_at_a_full_lane(&harness, &poster, &token, read).await;

    token.cancel();
    gate.notify_one();
    tokio::time::timeout(PROGRESS_BUDGET, running)
        .await
        .expect("a cancelled worker joins inside the supervisor's budget")
        .expect("the worker task must not panic");

    assert_eq!(
        poster.delivered(),
        vec!["1700000000000-0".to_owned()],
        "only the job in hand is delivered; the lane starts nothing after the \
         shutdown"
    );
    assert_eq!(
        harness.pending_count().await,
        read - 1,
        "the job in hand is acknowledged, and every other job the reader took \
         stays pending"
    );
    assert_eq!(
        harness.pending_consumers().await,
        vec![afd_dragonfly::outbound_consumer()],
        "pending under the name the next process comes back to"
    );
}

/// Starts a worker over `poster` and returns once a full lane holds its
/// reader, with `read` entries taken and one delivery in hand.
async fn held_at_a_full_lane(
    harness: &OutboundHarness,
    poster: &Gated,
    token: &CancellationToken,
    read: usize,
) -> tokio::task::JoinHandle<()> {
    let worker = Worker::new(
        harness.reader().await,
        harness.queue.clone(),
        harness.database.clone(),
        Posters {
            slack: poster.clone(),
        },
    );
    let running = tokio::spawn(worker.run(token.clone()));

    await_until(
        "the lane to fill and its hand-off to hold the reader",
        async || poster.in_flight() == 1 && harness.pending_count().await == read,
    )
    .await;
    tokio::time::sleep(HAND_OFF_SETTLE).await;
    assert_eq!(
        harness.pending_count().await,
        read,
        "a full lane holds the reader: the entry past the waiting hand-off is \
         never read"
    );
    running
}
