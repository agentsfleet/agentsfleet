//! What the worker does when its read keeps failing.
//!
//! The failure this guards against shipped: a dedicated connection whose reply
//! deadline was shorter than the park it asked for failed EVERY read, instantly
//! and forever, and the loop re-read with nothing between the turns. One
//! deployment logged the same warning about two hundred and sixty times a
//! second, on a task sharing its runtime with every request handler in the
//! process — a Dragonfly blip became a busy loop that outlived it.
//!
//! So a failing read has to cost time, and a shutdown must not have to wait
//! that time out. Both are asserted here, without a Dragonfly: a server that hangs
//! up on the read is all it takes to hold the loop in its failing branch.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::{Dedicated, Dragonfly, OutboundDelivery, OutboundQueue, OutboundReader};
use afd_outbound::{Deliver, LONGEST_PARK, Posters, Verdict, Worker};
use tokio_util::sync::CancellationToken;

#[path = "support/hanging_queue.rs"]
#[allow(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#[allow(
    dead_code,
    reason = "shared support: the read suite grades the reads, the lane suite the acks"
)]
mod hanging_queue;
#[path = "support/no_ledger.rs"]
mod no_ledger;

use self::hanging_queue::HangingQueue;

/// The window the spin is measured over.
///
/// Shorter than [`LONGEST_PARK`], which is the point: one backoff has not
/// elapsed, so a worker that pauses can only have read once or twice, and one
/// that spins has read hundreds of times.
const SPIN_WINDOW: Duration = Duration::from_millis(1_500);

/// Reads the window may contain: one worker turn, plus the driver's own retry
/// ladder beneath it.
///
/// The fake HANGS UP on every read, which is a connection fault — so the
/// cluster driver redials and retries the command before the failure ever
/// reaches the worker. `afd_dragonfly::transport` configures that ladder at
/// eight retries, making `1 + 8` the most one worker turn can put on the wire.
/// Measured at five here: eleven `HELLO`s and twenty-two `CLIENT SETINFO`s for
/// those five reads, which is the redial, not the worker.
///
/// This bound still proves what the test is named for. A worker that PARKED
/// takes one turn in this window; a worker that SPUN takes hundreds of turns
/// and hundreds of ladders with them, so the two are nowhere near each other.
/// Counting server-side reads as worker reads was only ever right while the
/// client had no retries of its own.
const MAX_READS_IN_WINDOW: usize = 9;

/// How long a cancelled worker may take to stop.
///
/// Well inside [`LONGEST_PARK`]: a shutdown that had to wait out the backoff
/// would blow the supervisor's join budget every time Dragonfly was unwell.
const SHUTDOWN_BUDGET: Duration = Duration::from_millis(500);

/// How long a fresh worker may take to send its first read.
const READ_REACHED_WITHIN: Duration = Duration::from_secs(2);

/// The connection's own allowance for an answer to travel.
const REQUEST_DEADLINE: Duration = Duration::from_millis(100);

/// A poster that must never be reached: no read succeeds, so no job exists.
///
/// It COUNTS rather than panicking. A panic here would land inside the spawned
/// worker task, where it surfaces as a join failure naming the task instead of
/// the invariant — a counter read back on the test's own thread says which
/// assertion broke.
#[derive(Debug, Clone)]
struct Unreachable {
    calls: Arc<AtomicUsize>,
}

impl Deliver for Unreachable {
    fn deliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.calls.fetch_add(1, Ordering::AcqRel);
        std::future::ready(Verdict::Permanent)
    }
}

/// A worker reading from `server`, the token that stops it, and the count of
/// deliveries it attempted.
async fn worker_against(
    server: &HangingQueue,
) -> (Worker<Unreachable>, CancellationToken, Arc<AtomicUsize>) {
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(REQUEST_DEADLINE);
    let redis = Dragonfly::connect(&config)
        .await
        .expect("the fake queue answers a ping");
    let connection = Dedicated::connect(&config, LONGEST_PARK)
        .await
        .expect("the fake queue accepts a connection");
    let calls = Arc::new(AtomicUsize::new(0));
    let worker = Worker::new(
        OutboundReader::new(connection, "read-backoff-probe".to_owned()),
        OutboundQueue::new(redis),
        no_ledger::no_ledger(),
        Posters {
            slack: Unreachable {
                calls: Arc::clone(&calls),
            },
        },
    );
    (worker, CancellationToken::new(), calls)
}

/// A read that fails does not immediately re-read.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_failing_read_is_not_retried_in_a_spin() {
    let server = HangingQueue::spawn().await;
    let (worker, token, delivered) = worker_against(&server).await;
    let running = tokio::spawn(worker.run(token.clone()));

    tokio::time::sleep(SPIN_WINDOW).await;
    let reads = server.reads();
    token.cancel();
    running.await.expect("the worker task must not panic");

    assert_eq!(
        delivered.load(Ordering::Acquire),
        0,
        "no read succeeded, so the worker had nothing to deliver"
    );
    assert!(
        reads > 0,
        "the worker never read at all, so this proves nothing about its backoff"
    );
    assert!(
        reads <= MAX_READS_IN_WINDOW,
        "the worker read {reads} times in {SPIN_WINDOW:?} — more than one turn's \
         worth, so a failing read is being retried in a spin rather than paused \
         for {LONGEST_PARK:?}"
    );
}

/// A cancelled worker stops inside a STALLED read, rather than waiting it out.
///
/// The resume read asks for no `BLOCK`, so it reads as instant — but its reply
/// allowance is now the longest park plus the request timeout, because that is
/// what `Dedicated::connect` sets for the blocking read that shares the
/// connection. A peer that accepts the socket and answers nothing therefore
/// holds this read for the whole allowance.
///
/// That is the regression this guards: raising the deadline to make the park
/// work also made an unraced await on this read outlast
/// `supervisor::JOIN_TIMEOUT`, which reports the task abandoned on a stop that
/// was otherwise clean. Unlike the backoff test below, this one discriminates
/// the bug — with the read unraced it takes the full allowance, not merely
/// longer than it should.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_shutdown_does_not_wait_out_a_stalled_read() {
    let server = HangingQueue::spawn_stalling().await;
    let (worker, token, delivered) = worker_against(&server).await;
    let running = tokio::spawn(worker.run(token.clone()));

    // Once the stalling peer has been asked for a read, the worker is parked
    // in it: the only state where this assertion means anything.
    let deadline = Instant::now() + READ_REACHED_WITHIN;
    while server.reads() == 0 {
        assert!(
            Instant::now() < deadline,
            "the worker never reached the read, so this proves nothing about shutdown"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let started = Instant::now();
    token.cancel();
    running.await.expect("the worker task must not panic");
    let stopped = started.elapsed();

    assert_eq!(
        delivered.load(Ordering::Acquire),
        0,
        "no read succeeded, so the worker had nothing to deliver"
    );
    assert!(
        stopped < SHUTDOWN_BUDGET,
        "the worker took {stopped:?} to stop against a stalled peer — the resume \
         read is being awaited rather than raced against the token, so shutdown \
         waits out the {LONGEST_PARK:?} reply allowance"
    );
}

/// A cancelled worker stops inside the pause, rather than waiting it out.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_shutdown_does_not_wait_out_the_backoff() {
    let server = HangingQueue::spawn().await;
    let (worker, token, delivered) = worker_against(&server).await;
    let running = tokio::spawn(worker.run(token.clone()));

    // Long enough that the first read has failed and the loop is inside the
    // pause — which is the only state where this assertion means anything.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let started = Instant::now();
    token.cancel();
    running.await.expect("the worker task must not panic");
    let stopped = started.elapsed();
    assert_eq!(
        delivered.load(Ordering::Acquire),
        0,
        "no read succeeded, so the worker had nothing to deliver"
    );

    assert!(
        stopped < SHUTDOWN_BUDGET,
        "the worker took {stopped:?} to stop — the backoff is being slept through \
         rather than raced against the token"
    );
}

/// The failures a capture holds of a read for new entries, oldest first.
fn new_read_failures(capture: &afd_core::test_util::trace::Capture) -> usize {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some("outbound_read_next_failed"))
        .count()
}

/// A blocking read that fails after the resume read came back empty is
/// reported under its own event, waited out for the whole backoff, and then
/// tried again; a shutdown during the next pause stops at once.
///
/// The other backoff cases fail the RESUME read, which keeps the worker
/// resuming; this one is the steady state's failure, past the hand-over.
/// Current-thread, so the worker's events land in this thread's capture.
#[tokio::test]
async fn test_a_failed_read_for_new_entries_is_retried_after_the_backoff() {
    let capture = afd_core::test_util::trace::Capture::install();
    let server = HangingQueue::spawn_failing_new_reads().await;
    let (worker, token, delivered) = worker_against(&server).await;
    let running = tokio::spawn(worker.run(token.clone()));

    let deadline = Instant::now() + LONGEST_PARK * 3;
    let mut first_failed = None;
    while new_read_failures(&capture) < 2 {
        assert!(
            Instant::now() < deadline,
            "the failing read was not retried: {:?}",
            capture.events()
        );
        if first_failed.is_none() && new_read_failures(&capture) == 1 {
            first_failed = Some(Instant::now());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let retried_after = first_failed.map(|at| at.elapsed());
    let started = Instant::now();
    token.cancel();
    running.await.expect("the worker task must not panic");

    // Half the backoff, not all of it: the first failure is seen by a 5 ms
    // poll, and the claim is a pause rather than a spin, which retries in
    // microseconds.
    assert!(
        retried_after.is_some_and(|gap| gap >= LONGEST_PARK / 2),
        "the retry came {retried_after:?} after the failure, inside the {LONGEST_PARK:?} backoff"
    );
    assert!(
        started.elapsed() < SHUTDOWN_BUDGET,
        "the pause after the failed read is slept through rather than raced"
    );
    assert_eq!(delivered.load(Ordering::Acquire), 0);
    assert!(
        !capture
            .events()
            .iter()
            .any(|event| event.field("event") == Some("outbound_read_pending_failed")),
        "the resume read answered nothing pending; it did not fail"
    );
}
