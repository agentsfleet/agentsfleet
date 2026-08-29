//! Dimensions 5.1 and 5.2 against a live Redis, through `Worker::run`.
//!
//! `tests/delivery.rs` grades the retry POLICY without a server, because no
//! server can make a vendor answer 429 three times on demand. What it cannot
//! reach is everything the policy is wrapped in: the consumer group, the
//! pending list, the acknowledgement, and the loop that ties them together.
//! `Worker::run` needs a real `OutboundReader` — an owned socket parked on
//! `XREADGROUP` — and there is no in-memory stand-in for one that would prove
//! anything about a consumer group. So these run here.
//!
//! # What each dimension is actually asking
//!
//! **5.1** — a queued answer is delivered once; a destination that keeps
//! failing is offered the budget and no more, then handled terminally. The
//! terminal half is the interesting one: an exhausted job is ACKNOWLEDGED, not
//! left pending, because delivery is serial and one undeliverable answer left
//! at the head of the queue would stop every answer behind it forever.
//!
//! **5.2** — a shutdown mid-delivery loses nothing and duplicates nothing.
//! Proven in two halves that have to be one test, because the second half's
//! whole claim is that it inherits the first half's pending entry: a worker
//! stopped mid-delivery leaves the entry unacknowledged, and the NEXT worker's
//! pending-first read is what finds it. Splitting them would leave the second
//! asserting against state it set up for itself.
//!
//! # Serialised, and why that is not a smell here
//!
//! `connector:outbound` and `connector_workers` are constants shared with the
//! Zig daemon, so these tests cannot namespace their key the way every other
//! integration suite does — they would be grading a stream production never
//! reads. They take `OUTBOUND_LANE` one at a time instead. See the harness.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use afd_outbound::retry::DELIVERY_ATTEMPTS;
use afd_outbound::{Deliver, Posters, Verdict, Worker};
use afd_redis::{OutboundDelivery, OutboundJob};
use tokio_util::sync::CancellationToken;

#[path = "support/outbound_harness.rs"]
mod support;

use self::support::{OUTBOUND_LANE, OutboundHarness};

/// How long a worker may take to pick a job up, deliver it, and acknowledge it.
///
/// Generous against a cold container and a jittered backoff — the assertions
/// are about counts, and a budget tight enough to be flaky would grade the
/// lane's disk rather than the worker.
const PROGRESS_BUDGET: Duration = Duration::from_secs(15);

/// How often the test asks whether the worker has got there yet.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The floor the two backoff sleeps of an exhausted delivery cannot go under.
///
/// `retry::delivery_schedule` starts at 200ms and doubles, so three attempts
/// sleep 200ms + 400ms before jitter, and `with_jitter` only ever ADDS. 500ms
/// leaves headroom for timer granularity while still being far above the zero
/// a worker that had stopped backing off would post.
const MIN_BACKOFF_WAIT: Duration = Duration::from_millis(500);

/// A workspace id shaped like the ones the report path queues.
const WORKSPACE_ID: &str = "0199a0b0-0000-7000-8000-000000000001";
/// See [`WORKSPACE_ID`].
const FLEET_ID: &str = "0199a0b0-0000-7000-8000-000000000002";
/// The provider `dispatch` routes to the Slack poster, which is the one arm
/// with a poster behind it.
const PROVIDER: &str = "slack";

/// What one scripted poster remembers, behind whatever handles point at it.
#[derive(Debug)]
struct Script {
    /// Drained per attempt, its LAST entry repeating — so a test asserting the
    /// attempt ceiling is not accidentally asserting the length of its own
    /// script. A destination that is down stays down.
    answers: Mutex<Vec<Verdict>>,
    attempts: AtomicUsize,
    /// Answers seen, in order, so a test can assert WHICH job was delivered
    /// rather than only how many times something was.
    seen: Mutex<Vec<String>>,
    /// Cancels the supervisor's token from inside an attempt, which is the only
    /// way to reach "shutdown arrived mid-delivery" deterministically.
    cancel_on: Option<(usize, CancellationToken)>,
}

/// A poster that answers from a script and records what it was asked.
///
/// # Why this is a cloneable handle rather than the state itself
///
/// `Worker::new` takes its posters BY VALUE — correctly, since a worker owns
/// them for its whole run — and every assertion here is about what the poster
/// saw. Both need to hold it. The shared half is behind one `Arc` inside the
/// handle rather than the test wrapping the poster in an `Arc` of its own,
/// because `Deliver` is this crate's trait and `Arc` is not this crate's type:
/// the orphan rule refuses `impl Deliver for Arc<Scripted>` from a test target.
#[derive(Debug, Clone)]
struct Scripted {
    script: std::sync::Arc<Script>,
}

impl Scripted {
    /// A poster that answers `answers`, the last repeating.
    fn new(answers: &[Verdict]) -> Self {
        Self::build(answers, None)
    }

    /// As [`Self::new`], but cancels `token` at the start of attempt `on`.
    fn cancelling(answers: &[Verdict], on: usize, token: CancellationToken) -> Self {
        Self::build(answers, Some((on, token)))
    }

    /// The one constructor both shapes go through.
    fn build(answers: &[Verdict], cancel_on: Option<(usize, CancellationToken)>) -> Self {
        assert!(!answers.is_empty(), "a script needs at least one answer");
        Self {
            script: std::sync::Arc::new(Script {
                // Reversed so each call is a `pop` off the end rather than a
                // remove from the front, which would be O(n) per attempt for
                // no reason.
                answers: Mutex::new(answers.iter().copied().rev().collect()),
                attempts: AtomicUsize::new(0),
                seen: Mutex::new(Vec::new()),
                cancel_on,
            }),
        }
    }

    /// How many attempts this poster was asked for.
    fn attempts(&self) -> usize {
        self.script.attempts.load(Ordering::Relaxed)
    }

    /// The answers this poster was handed, in the order it saw them.
    fn seen(&self) -> Vec<String> {
        self.script
            .seen
            .lock()
            .expect("no test panics holding this")
            .clone()
    }
}

impl Deliver for Scripted {
    fn deliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        let index = self.script.attempts.fetch_add(1, Ordering::Relaxed);
        self.script
            .seen
            .lock()
            .expect("no test panics holding this")
            .push(job.answer.clone());
        if let Some((on, token)) = &self.script.cancel_on
            && index == *on
        {
            token.cancel();
        }

        let mut answers = self
            .script
            .answers
            .lock()
            .expect("no test panics holding this");
        let answer = if answers.len() > 1 {
            answers.pop().expect("length checked above")
        } else {
            *answers.last().expect("a script is never empty")
        };
        std::future::ready(answer)
    }
}

/// Polls `condition` until it holds or [`PROGRESS_BUDGET`] runs out.
///
/// A poll rather than a channel because what is being waited on is the worker's
/// EFFECT — an acknowledgement in Redis, a counter in a poster — and wiring a
/// signal into the worker to observe it would be testing the signal. `note`
/// names what was being waited for, so a timeout says which claim failed rather
/// than that a duration elapsed.
async fn await_until<F>(note: &str, mut condition: F)
where
    F: AsyncFnMut() -> bool,
{
    let deadline = Instant::now() + PROGRESS_BUDGET;
    while Instant::now() < deadline {
        if condition().await {
            return;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    panic!("timed out after {PROGRESS_BUDGET:?} waiting for {note}");
}

/// Queues one answer, returning nothing: the id comes back off the stream.
async fn enqueue(harness: &OutboundHarness, answer: &str) {
    harness
        .queue
        .enqueue(OutboundJob {
            provider: PROVIDER,
            workspace_id: WORKSPACE_ID,
            fleet_id: FLEET_ID,
            event_id: "1700000000000-0",
            answer,
        })
        .await
        .expect("the lane's Redis must accept an enqueue");
}

/// Dimension 5.1 — an answer is delivered once, and a failing destination is
/// offered the budget and no more before terminal handling.
///
/// Both jobs go through ONE worker in one run, deliberately: the claim is not
/// only that each is handled correctly but that a job whose delivery exhausted
/// its budget does not wedge the serial queue behind it. A worker that left the
/// exhausted job pending would re-read it forever and the delivered job's
/// assertion would time out — which is the failure this arrangement catches and
/// two separate tests would not.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_outbound_delivery_retry() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;

    let doomed = "the destination is down";
    let fine = "the destination answers";
    enqueue(&harness, doomed).await;
    enqueue(&harness, fine).await;

    // Retryable first and Delivered after: the script is drained per ATTEMPT,
    // so the doomed job's three attempts take the three Retryables and the
    // second job's single attempt takes the Delivered that repeats after them.
    let poster = Scripted::new(&[
        Verdict::Retryable,
        Verdict::Retryable,
        Verdict::Retryable,
        Verdict::Delivered,
    ]);
    let posters = Posters {
        slack: poster.clone(),
    };

    let token = CancellationToken::new();
    let worker = Worker::new(harness.reader().await, harness.queue.clone(), posters);
    let started = Instant::now();
    let running = tokio::spawn(worker.run(token.clone()));

    // Both halves of the condition are load-bearing. `pending_count == 0` alone
    // is true in the gap between the first job being acknowledged and the
    // second being read — nothing is pending because nothing has been handed
    // out — so a wait on it would return with the second job still in the
    // stream and every assertion below would grade half a run.
    await_until("both jobs to be delivered and acknowledged", async || {
        poster.attempts() == DELIVERY_ATTEMPTS + 1 && harness.pending_count().await == 0
    })
    .await;
    let elapsed = started.elapsed();

    token.cancel();
    running.await.expect("the worker task must not panic");

    assert_eq!(
        poster.attempts(),
        DELIVERY_ATTEMPTS + 1,
        "the doomed job is offered its whole budget and the healthy job once; \
         more means the exhausted job was redelivered, fewer means the budget \
         was cut short"
    );
    assert_eq!(
        poster.seen(),
        vec![
            doomed.to_owned(),
            doomed.to_owned(),
            doomed.to_owned(),
            fine.to_owned(),
        ],
        "delivery is serial: the second answer is not started until the first \
         is finished with"
    );
    assert_eq!(
        harness.pending_count().await,
        0,
        "terminal handling — an exhausted delivery is acknowledged, not left \
         at the head of a serial queue to be redelivered forever"
    );
    assert!(
        elapsed >= MIN_BACKOFF_WAIT,
        "three attempts in {elapsed:?} means the retry stopped sleeping: the \
         un-jittered schedule alone is 200ms + 400ms, and jitter only adds"
    );
}

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
#[ignore = "needs live Redis: make test-integration-rustd"]
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

    let worker = Worker::new(harness.reader().await, harness.queue.clone(), interrupted);
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
        vec![afd_redis::outbound_consumer()],
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

    let worker = Worker::new(harness.reader().await, harness.queue.clone(), resumed);
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
#[ignore = "needs live Redis: make test-integration-rustd"]
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

    let worker = Worker::new(harness.reader().await, harness.queue.clone(), posters);
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

/// `Worker::run` creates its own consumer group rather than reading into
/// `NOGROUP` forever.
///
/// The heal is in `run` and not only at boot for a reason worth grading: a
/// failover onto an empty replica loses the group while the process keeps
/// running, and a worker that only ever created it at startup would then log a
/// `NOGROUP` per read for the life of the deployment with every answer queuing
/// up behind it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_the_worker_creates_the_group_it_reads_under() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset_without_group().await;

    let token = CancellationToken::new();
    let poster = Scripted::new(&[Verdict::Delivered]);
    let posters = Posters {
        slack: poster.clone(),
    };

    let worker = Worker::new(harness.reader().await, harness.queue.clone(), posters);
    let running = tokio::spawn(worker.run(token.clone()));

    // Queued AFTER the worker started, so the group it reads under can only be
    // one the worker itself created — the enqueue does not make one.
    let answer = "queued onto a stream that had no group";
    enqueue(&harness, answer).await;

    await_until("the answer to be delivered and acknowledged", async || {
        harness.pending_count().await == 0 && poster.attempts() == 1
    })
    .await;

    token.cancel();
    running.await.expect("the worker task must not panic");

    assert_eq!(
        poster.seen(),
        vec![answer.to_owned()],
        "the worker healed the missing group and delivered through it"
    );
}
