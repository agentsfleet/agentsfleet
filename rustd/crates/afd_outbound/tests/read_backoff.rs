//! What the worker does when its read keeps failing.
//!
//! The failure this guards against shipped: a dedicated connection whose reply
//! deadline was shorter than the park it asked for failed EVERY read, instantly
//! and forever, and the loop re-read with nothing between the turns. One
//! deployment logged the same warning about two hundred and sixty times a
//! second, on a task sharing its runtime with every request handler in the
//! process — a Redis blip became a busy loop that outlived it.
//!
//! So a failing read has to cost time, and a shutdown must not have to wait
//! that time out. Both are asserted here, without a Redis: a server that hangs
//! up on the read is all it takes to hold the loop in its failing branch.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use afd_outbound::{Deliver, LONGEST_PARK, Posters, Verdict, Worker};
use afd_redis::config::{RedisConfig, RedisRole};
use afd_redis::{Dedicated, OutboundDelivery, OutboundQueue, OutboundReader, Redis};
use tokio_util::sync::CancellationToken;

#[path = "support/hanging_queue.rs"]
#[allow(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod hanging_queue;

use self::hanging_queue::HangingQueue;

/// The window the spin is measured over.
///
/// Shorter than [`LONGEST_PARK`], which is the point: one backoff has not
/// elapsed, so a worker that pauses can only have read once or twice, and one
/// that spins has read hundreds of times.
const SPIN_WINDOW: Duration = Duration::from_millis(1_500);

/// Reads the window may contain: the first, and one more for a turn already in
/// flight when the window opened.
const MAX_READS_IN_WINDOW: usize = 2;

/// How long a cancelled worker may take to stop.
///
/// Well inside [`LONGEST_PARK`]: a shutdown that had to wait out the backoff
/// would blow the supervisor's join budget every time Redis was unwell.
const SHUTDOWN_BUDGET: Duration = Duration::from_millis(500);

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
    let config = RedisConfig::from_url(RedisRole::Default, server.url())
        .with_request_timeout(REQUEST_DEADLINE);
    let redis = Redis::connect(&config)
        .await
        .expect("the fake queue answers a ping");
    let connection = Dedicated::connect(&config, LONGEST_PARK)
        .await
        .expect("the fake queue accepts a connection");
    let calls = Arc::new(AtomicUsize::new(0));
    let worker = Worker::new(
        OutboundReader::new(connection, "read-backoff-probe".to_owned()),
        OutboundQueue::new(redis),
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
        "the worker read {reads} times in {SPIN_WINDOW:?} — a failing read is being \
         retried in a spin rather than paused for {LONGEST_PARK:?}"
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

    // Long enough that the worker is parked in the resume read, which is the
    // only state where this assertion means anything.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        server.reads() > 0,
        "the worker never reached the read, so this proves nothing about shutdown"
    );
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
