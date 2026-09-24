//! Dimensions 5.1 and 5.2 against a live Dragonfly, through `Worker::run`.
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

use afd_dragonfly::{OutboundDelivery, OutboundJob};
use afd_outbound::retry::DELIVERY_ATTEMPTS;
use afd_outbound::{Deliver, Posters, Verdict, Worker};
use tokio_util::sync::CancellationToken;

#[path = "support/outbound_harness.rs"]
mod support;

use self::support::{OUTBOUND_LANE, OutboundHarness};

/// The thread an owed answer is addressed to, as a Slack producer records it.
const DESTINATION: &str = r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

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
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
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
    let worker = Worker::new(
        harness.reader().await,
        harness.queue.clone(),
        harness.database.clone(),
        posters,
    );
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

/// `Worker::run` creates its own consumer group rather than reading into
/// `NOGROUP` forever.
///
/// The heal is in `run` and not only at boot for a reason worth grading: a
/// failover onto an empty replica loses the group while the process keeps
/// running, and a worker that only ever created it at startup would then log a
/// `NOGROUP` per read for the life of the deployment with every answer queuing
/// up behind it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_the_worker_creates_the_group_it_reads_under() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset_without_group().await;

    let token = CancellationToken::new();
    let poster = Scripted::new(&[Verdict::Delivered]);
    let posters = Posters {
        slack: poster.clone(),
    };

    let worker = Worker::new(
        harness.reader().await,
        harness.queue.clone(),
        harness.database.clone(),
        posters,
    );
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

/// An entry nothing can decode is dropped, not re-offered forever.
///
/// The failure this exists to stop is a queue-wide stall. `decode` answers
/// `None` for an entry short of a field it requires, and answering the caller
/// "nothing pending" would leave that entry PENDING under this consumer — so
/// every later [`OutboundReader::read_pending`] hands back the same row, and
/// every answer queued behind it waits on one row nothing can deliver. A
/// single write by operator tooling or by the Zig sharing this key would stop
/// outbound answers for the whole deployment.
///
/// The first read is the BLOCKING one deliberately. `read_pending` reads what
/// this consumer has already been handed, and a freshly written entry has been
/// handed to nobody — so opening on `read_pending` would assert against an
/// empty pending list and pass whether or not the fix is present. The `>` read
/// is what assigns the entry to this consumer, and assignment is what makes it
/// pending; only then is there something for the acknowledgement to drain.
#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn an_entry_that_cannot_be_decoded_is_acknowledged_rather_than_re_offered() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;
    harness.poison().await;

    let mut reader = harness.reader().await;
    let read = reader
        .read_blocking(500)
        .await
        .expect("a reachable queue answers the read");
    assert!(
        read.is_none(),
        "an entry short of a required field is not a delivery"
    );
    assert_eq!(
        harness.pending_count().await,
        0,
        "the undeliverable entry must leave the pending list — left there, a \
         pending-first read hands it back every turn and every job behind it \
         waits forever"
    );

    // And the queue still works behind it: the next real job is deliverable.
    enqueue(&harness, "Aurora is healthy.").await;
    let next = reader
        .read_blocking(500)
        .await
        .expect("a reachable queue answers the read");
    assert_eq!(
        next.map(|delivery| delivery.answer),
        Some("Aurora is healthy.".to_owned()),
        "the job queued behind the poisoned entry must still be delivered"
    );
}

/// A queue that answers and refuses is this daemon's fault, not an outage.
///
/// `afd_outbound::Error::code` splits its one variant two ways, and the split
/// is what an operator acts on: a queue that is GONE is
/// `INTERNAL_DB_UNAVAILABLE` — retry it, page the infrastructure — while a
/// queue that answered and said no is `INTERNAL_OPERATION_FAILED`, a defect
/// here. Collapsing them sends somebody to check a healthy Dragonfly over a bug in
/// this crate. `worker.rs` reads that code onto every failure it reports, so
/// the mapping is what an incident is triaged from.
///
/// # Why the error is provoked and then LIFTED
///
/// `afd_dragonfly` builds its kinds crate-privately, so the non-outage case cannot
/// be constructed by hand — only caused, which is what the wrong-typed key
/// does. But the queue's own methods return `afd_dragonfly::Result`, so asserting
/// on what `enqueue` hands back grades THAT crate's mapping and never reaches
/// this one's. The lift is the step that crosses the boundary, and it is the
/// same `From` the worker's `?` uses on the same value.
#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_queue_that_answers_and_refuses_is_not_reported_as_an_outage() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = OutboundHarness::reset().await;
    harness.clobber_with_a_string().await;

    let refused = harness
        .queue
        .enqueue(OutboundJob {
            provider: PROVIDER,
            destination: DESTINATION,
            workspace_id: WORKSPACE_ID,
            fleet_id: FLEET_ID,
            event_id: "1700000000-0",
            answer: "Aurora is healthy.",
        })
        .await
        .expect_err("a stream key holding a string cannot take an entry");

    assert!(
        !refused.is_unavailable(),
        "a server that answered WRONGTYPE is reachable — treating it as an \
         outage is what puts this on the wrong side of the split"
    );

    let reported: afd_outbound::Error = refused.into();
    assert_eq!(
        reported.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED,
        "the queue answered — that is a defect here, not the outage an \
         operator retries against"
    );
}

#[path = "integration_worker/scripted.rs"]
mod scripted;
#[path = "integration_worker/shutdown.rs"]
mod shutdown;

use self::scripted::{Scripted, await_until, enqueue};
