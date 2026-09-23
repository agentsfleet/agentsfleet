//! What `attempt_count` counts, now that it counts attempts.
//!
//! The counter used to move inside the success stamp, and the only branch that
//! reaches the stamp is the delivered one. So a destination that refused an
//! answer nine times and took it on the tenth recorded `1`, and one that never
//! took it recorded `0` — the row an operator most needs to find looked exactly
//! like a row nobody had tried. The worker now records a cycle START, before
//! the verdict exists, and this file grades the definition that came with it:
//!
//! - one `deliver_with_retry` cycle is one count, however many vendor retries
//!   it made inside;
//! - a cycle that ends in failure still counts, and the next cycle counts again
//!   while the obligation stays undelivered;
//! - the success stamp moves `delivered_at` once and the counter never;
//! - the count rides the worker's own structured events, delivered and
//!   exhausted alike;
//! - a ledger that will not take the count does not cost anybody the answer.
//!
//! # Two kinds of test, one file
//!
//! The tests that grade the LEDGER drive `count_attempt` and the scans
//! directly against the shared schema, the way `integration_obligations.rs`
//! does. The tests that grade the WORKER drive `Lanes` with a scripted poster,
//! the way `lanes.rs` does, and take their ledger from the harness so the
//! statement under test actually runs — except the one about bookkeeping
//! failure, which takes the unreachable ledger on purpose.
//!
//! # Serialised on the shared stream, like its neighbours
//!
//! Every test here that opens the harness takes [`OUTBOUND_LANE`], for the
//! reason `support/outbound_harness.rs` gives. This is its own test binary, so
//! the tracing subscriber it installs is this file's alone — which is what lets
//! it CAPTURE the worker's events rather than merely evaluate them.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::streams::EventId;
use afd_dragonfly::{Dragonfly, OutboundDelivery, OutboundJob, OutboundQueue};
use afd_outbound::obligation::{self, AbandonReason, Delivery};
use afd_outbound::producer::MAX_DELIVERY_CYCLES;
use afd_outbound::retry::DELIVERY_ATTEMPTS;
use afd_outbound::{Deliver, Lanes, Posters, Verdict};
use tokio_util::sync::CancellationToken;
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt as _};

#[path = "support/hanging_queue.rs"]
#[allow(
    clippy::expect_used,
    dead_code,
    reason = "shared support: this suite needs the ack log, not the read counter"
)]
mod hanging_queue;
#[path = "support/no_ledger.rs"]
mod no_ledger;
#[path = "support/obligation_seed.rs"]
#[allow(
    dead_code,
    reason = "shared seed: this suite owes and counts, it does not forget streams"
)]
mod seed;
#[path = "support/outbound_harness.rs"]
#[allow(
    dead_code,
    reason = "one harness, several binaries: this one drives the ledger and the lanes"
)]
mod support;

use self::hanging_queue::HangingQueue;
use self::seed::{FLEET, SEEDED_AT, WORKSPACE, clear_obligations, obligation_id, seed_parents};
use self::support::{OUTBOUND_LANE, OutboundHarness};

/// The connector every fixture answer goes back through.
const PROVIDER: &str = Provider::Slack.id();

/// The thread every owed answer here is addressed to.
const DESTINATION: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;
/// What the fixture answers say.
const ANSWER: &str = "Aurora is healthy.";
/// More rows than any test seeds, so a scan's limit never decides an assertion.
const AMPLE: i64 = 64;
/// How long a lane is given to reach a terminal verdict and acknowledge.
///
/// Three attempts with two jittered sleeps come to under a second and a half;
/// the rest is the coverage lane's oversubscription, which `lanes.rs` measured
/// and gave thirty seconds for the same reason.
const PATIENCE: Duration = Duration::from_secs(30);
/// The gap between two looks at a condition.
const POLL: Duration = Duration::from_millis(5);
/// The deadline the client gives the fake queue for any one command.
const REQUEST_DEADLINE: Duration = Duration::from_secs(2);
/// The structured events the worker emits and this file listens for.
const EVENT_DELIVERED: &str = "outbound_delivery_delivered";
const EVENT_EXHAUSTED: &str = "outbound_delivery_exhausted";
const EVENT_REQUEUED: &str = "outbound_delivery_requeued_at_shutdown";
const EVENT_COUNT_FAILED: &str = "outbound_obligation_attempt_failed";
const EVENT_STAMP_FAILED: &str = "outbound_obligation_stamp_failed";
const EVENT_ABANDONED: &str = "outbound_delivery_abandoned";
/// The field the count rides on.
const FIELD_ATTEMPTS: &str = "attempts";
/// The field an event is named by.
const FIELD_EVENT: &str = "event";

/// How long after the fixture instant the graded delivery cycle starts.
///
/// Any gap does: the scan takes its cutoff as a parameter, so these tests move
/// the cutoff rather than the clock. What the two numbers have to be is
/// ORDERED — a cutoff before the cycle start, and the start itself — because
/// the claim is about which side of the start a cutoff falls on.
const CYCLE_STARTS_AFTER: i64 = 1_000;

/// A cutoff earlier than that start.
const BEFORE_THE_CYCLE: i64 = 500;

// ── The ledger half ─────────────────────────────────────────────────────────

/// One owed answer, addressed.
fn delivery(event_id: &str) -> Delivery<'_> {
    Delivery {
        fleet_id: FLEET,
        workspace_id: WORKSPACE,
        provider: Provider::Slack,
        destination: DESTINATION,
        event_id,
        answer: ANSWER,
    }
}

/// A fixture in the state each test starts from: parents seeded, nothing owed.
///
/// Installs the capturing subscriber BEFORE the harness, and that order is the
/// whole reason this wrapper exists rather than calling `reset` directly.
/// `OutboundHarness::reset` installs a subscriber of its own that writes to a
/// sink, both are global, and `set_global_default` takes the first caller and
/// silently refuses the rest. Tests run in parallel, so whichever ran first
/// decided whether this file could read its own events — the ledger-half tests
/// here never ask for the capture, and when one of them reached the harness
/// first the worker-half tests found an empty log and failed. Going through
/// `capture()` on every path makes the first global subscriber in this binary
/// the capturing one, whatever order the tests start in.
async fn ready() -> OutboundHarness {
    capture();
    let harness = OutboundHarness::reset().await;
    seed_parents(&harness.database).await;
    clear_obligations(&harness.database).await;
    harness
}

/// Owes an answer, appends it and records the receipt — the report's fast path.
///
/// Answers the entry id the queue minted, which is what a job carries as its
/// `id` and what the lanes acknowledge by.
async fn owe_and_queue(harness: &OutboundHarness, nth: u8, event: &str) -> EventId {
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
        &obligation_id(nth),
        delivery(event),
        UnixMillis::from_millis(SEEDED_AT),
    )
    .await
    .expect("owing a delivery");
    transaction
        .commit()
        .await
        .expect("the report's transaction commits");
    assert!(written, "each fixture answer owes its own delivery");

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
        &obligation_id(nth),
        entry.as_str(),
        UnixMillis::from_millis(SEEDED_AT),
    )
    .await
    .expect("recording the receipt");
    entry
}

/// The row as the ledger holds it: `(attempt_count, delivered_at, updated_at)`.
async fn row(harness: &OutboundHarness, event: &str) -> (i64, Option<i64>, i64) {
    let mut connection = harness
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    sqlx::query_as(
        "SELECT attempt_count, delivered_at, updated_at FROM core.fleet_obligations
          WHERE fleet_id = $1::uuid AND event_id = $2::text",
    )
    .bind(FLEET)
    .bind(event)
    .fetch_one(&mut *connection)
    .await
    .expect("reading the obligation")
}

/// The event ids the recovery scan would re-offer before `cutoff`.
async fn reoffered_before(harness: &OutboundHarness, cutoff: i64) -> Vec<String> {
    obligation::undelivered(&harness.database, UnixMillis::from_millis(cutoff), AMPLE)
        .await
        .expect("the scan answers")
        .into_iter()
        .map(|owed| owed.event_id)
        .collect()
}

/// A cycle that ends in failure counts, and so does the next one.
///
/// Two starts recorded directly, the way the worker records them, with no
/// stamp in between: the row stays undelivered and reads `2`. Under the old
/// statement it would read `0` — indistinguishable from a row nobody had ever
/// picked up, which is the failure this counter existed to make visible and
/// could not.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn failed_cycles_increase_the_attempt_count() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000001-0";
    owe_and_queue(&harness, 1, event).await;

    let first = obligation::count_attempt(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(SEEDED_AT + 10),
    )
    .await
    .expect("counting the first cycle");
    let second = obligation::count_attempt(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(SEEDED_AT + 20),
    )
    .await
    .expect("counting the second cycle");

    assert_eq!(first, Some(1), "the first cycle start is the first attempt");
    assert_eq!(
        second,
        Some(2),
        "a second cycle on an undelivered row counts again"
    );
    let (attempts, delivered_at, updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 2);
    assert_eq!(delivered_at, None, "counting never stamps");
    assert_eq!(
        updated_at,
        SEEDED_AT + 20,
        "a cycle start moves updated_at, which is the recovery scan's clock"
    );
}

/// A redelivery of an answer somebody already received counts nothing.
///
/// The at-least-once edge, from the counter's side. The stamp lands once at
/// the first acceptance; a later cycle for the same obligation — a duplicate
/// queue entry, a lost acknowledgement — answers `None`, moves neither column,
/// and leaves `delivered_at` at the instant the destination FIRST took it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn duplicate_delivery_preserves_the_first_stamp() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000001-1";
    owe_and_queue(&harness, 2, event).await;

    let counted = obligation::count_attempt(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(SEEDED_AT + 10),
    )
    .await
    .expect("counting the cycle that delivers");
    assert_eq!(counted, Some(1));
    let accepted_at = SEEDED_AT + 15;
    obligation::stamp_delivered(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(accepted_at),
    )
    .await
    .expect("stamping the delivery");

    let again = obligation::count_attempt(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(SEEDED_AT + 30),
    )
    .await
    .expect("a duplicate cycle asks and is refused");
    obligation::stamp_delivered(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(SEEDED_AT + 35),
    )
    .await
    .expect("a duplicate stamp asks and is refused");

    assert_eq!(again, None, "an already-delivered row counts no cycle");
    let (attempts, delivered_at, updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 1, "the counter did not move for the duplicate");
    assert_eq!(
        delivered_at,
        Some(accepted_at),
        "the first acceptance stands"
    );
    assert_eq!(
        updated_at, accepted_at,
        "nothing after the stamp touched the row"
    );
}

/// A cycle start takes the row out of the recovery scan's window.
///
/// The pacing decision, proven in both directions. `SELECT_UNDELIVERED` asks
/// for rows untouched since its cutoff. A row a worker just accepted is being
/// worked on, so a cutoff BEFORE the start does not re-offer it; a cutoff after
/// the start does — which is the case of a worker that died mid-cycle and the
/// scan exists for.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn recovery_pacing_follows_the_cycle_start() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000001-2";
    owe_and_queue(&harness, 3, event).await;
    let started_at = SEEDED_AT + CYCLE_STARTS_AFTER;

    assert_eq!(
        reoffered_before(&harness, SEEDED_AT + BEFORE_THE_CYCLE).await,
        vec![event.to_owned()],
        "before any cycle starts, the queued row is the scan's business"
    );
    obligation::count_attempt(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(started_at),
    )
    .await
    .expect("counting the cycle");

    assert!(
        reoffered_before(&harness, started_at - 1).await.is_empty(),
        "a row whose cycle just started is somebody's work, not the scan's"
    );
    assert_eq!(
        reoffered_before(&harness, started_at + 1).await,
        vec![event.to_owned()],
        "once the window passes the start, a cycle that never finished is re-offered"
    );
}

// ── The worker half ─────────────────────────────────────────────────────────

/// A poster that answers from a script, one verdict per call.
///
/// The same shape `poster.rs`'s own `Counting` test double takes, with the
/// verdicts chosen by the test: three `Retryable`s exhaust the budget, two then
/// a `Delivered` prove the internal retries are one cycle, and a `Permanent`
/// proves a refusal is terminal on the first try.
#[derive(Debug, Clone)]
struct Scripted {
    verdicts: Arc<Vec<Verdict>>,
    calls: Arc<AtomicUsize>,
}

impl Scripted {
    fn answering(verdicts: &[Verdict]) -> Self {
        Self {
            verdicts: Arc::new(verdicts.to_vec()),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// How many times the poster was asked.
    fn calls(&self) -> usize {
        self.calls.load(Ordering::Acquire)
    }
}

impl Deliver for Scripted {
    fn deliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        let nth = self.calls.fetch_add(1, Ordering::AcqRel);
        let verdict = self
            .verdicts
            .get(nth)
            .or_else(|| self.verdicts.last())
            .copied()
            .unwrap_or(Verdict::Permanent);
        std::future::ready(verdict)
    }
}

/// One captured worker event: its name and the `attempts` it carried.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    event: String,
    attempts: Option<i64>,
}

/// Reads the two fields this file asserts on out of one event.
#[derive(Default)]
struct Fields {
    event: Option<String>,
    attempts: Option<i64>,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == FIELD_EVENT {
            self.event = Some(format!("{value:?}").trim_matches('"').to_owned());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == FIELD_EVENT {
            self.event = Some(value.to_owned());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == FIELD_ATTEMPTS {
            self.attempts = Some(value);
        }
    }
}

/// A layer that keeps every event the worker emits, for the test to read back.
#[derive(Debug, Default, Clone)]
struct Capture {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Capture {
    fn events(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn named(&self, event: &str) -> Vec<Seen> {
        self.events()
            .into_iter()
            .filter(|seen| seen.event == event)
            .collect()
    }
}

impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let Some(event) = fields.event {
            self.seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Seen {
                    event,
                    attempts: fields.attempts,
                });
        }
    }
}

/// Installs the capturing subscriber once, for this binary.
///
/// A global, because `tracing::warn!` asks its callsite whether it is enabled
/// before evaluating fields, and the events under test live in library code
/// that knows nothing about a test-scoped subscriber — the lanes deliver on
/// spawned tasks, so a thread-local scoped subscriber would miss them anyway.
/// One binary, one global, one capture shared by every test in it, which is why
/// every worker-half test filters by the event id it dispatched rather than by
/// position. Every path into the harness calls this FIRST; see [`ready`].
fn capture() -> Capture {
    static CAPTURE: std::sync::OnceLock<Capture> = std::sync::OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let capture = Capture::default();
            let subscriber = tracing_subscriber::registry().with(capture.clone());
            let _ = tracing::subscriber::set_global_default(subscriber);
            capture
        })
        .clone()
}

/// A job the lanes carry, addressed to the fixture fleet.
fn job(id: EventId, event_id: &str) -> Box<OutboundDelivery> {
    Box::new(OutboundDelivery {
        id,
        provider: PROVIDER.to_owned(),
        destination: DESTINATION.to_owned(),
        workspace_id: WORKSPACE.to_owned(),
        fleet_id: FLEET.to_owned(),
        event_id: event_id.to_owned(),
        answer: ANSWER.to_owned(),
    })
}

/// Lanes acknowledging through the fake queue and stamping into `database`.
async fn lanes_over(
    server: &HangingQueue,
    database: afd_db::Db,
    poster: Scripted,
    token: &CancellationToken,
) -> Lanes<Scripted> {
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(REQUEST_DEADLINE);
    let redis = Dragonfly::connect(&config)
        .await
        .expect("the fake queue answers a ping");
    Lanes::new(
        Posters { slack: poster },
        OutboundQueue::new(redis),
        database,
        token.clone(),
    )
}

/// Waits until `condition` holds, or fails the test naming what did not.
async fn await_until<F>(note: &str, mut condition: F)
where
    F: FnMut() -> bool,
{
    tokio::time::timeout(PATIENCE, async {
        while !condition() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .unwrap_or_else(|_elapsed| panic!("timed out waiting for {note}"));
}

/// One cycle is one count, however many vendor retries it made inside.
///
/// The definition's load-bearing half. A destination that answers 5xx twice
/// and then takes the answer made the poster work three times; the ledger
/// records ONE cycle, the stamp lands, and the delivered event carries that
/// count. A per-request counter would say three and would have cost a ledger
/// write on every rate-limit sleep to say it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn internal_retries_are_one_cycle_and_the_count_rides_the_delivered_event() {
    let capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000002-0";
    let entry = owe_and_queue(&harness, 4, event).await;
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Retryable, Verdict::Retryable, Verdict::Delivered]);
    let lanes = lanes_over(&server, harness.database.clone(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until("the delivered answer to be acknowledged", || {
        server.acks().contains(&entry.as_str().to_owned())
    })
    .await;
    token.cancel();
    lanes.drain().await;

    assert_eq!(poster.calls(), 3, "two refusals and an acceptance");
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(
        attempts, 1,
        "three vendor calls inside one cycle count once"
    );
    assert!(
        delivered_at.is_some(),
        "the destination took it, so it is stamped"
    );
    let delivered = capture.named(EVENT_DELIVERED);
    assert!(
        delivered.iter().any(|seen| seen.attempts == Some(1)),
        "the delivered event carries the recorded count: {delivered:?}"
    );
}

/// An exhausted cycle is counted, acknowledged, and reported with its count.
///
/// The row an operator is looking for. Three refusals spend the budget; the
/// job is acknowledged so it does not park at the head of the lane; the
/// obligation stays undelivered with a count of ONE — which under the old
/// statement would have read zero — and the exhausted warning names that count.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_exhausted_cycle_is_counted_and_reported() {
    let capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000002-1";
    let entry = owe_and_queue(&harness, 5, event).await;
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Retryable]);
    let lanes = lanes_over(&server, harness.database.clone(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until("the exhausted job to be acknowledged", || {
        server.acks().contains(&entry.as_str().to_owned())
    })
    .await;
    token.cancel();
    lanes.drain().await;

    assert_eq!(
        poster.calls(),
        DELIVERY_ATTEMPTS,
        "the whole budget was spent"
    );
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 1, "a failed cycle is still a cycle");
    assert_eq!(
        delivered_at, None,
        "nothing was delivered, so nothing is stamped"
    );
    let exhausted = capture.named(EVENT_EXHAUSTED);
    assert!(
        exhausted.iter().any(|seen| seen.attempts == Some(1)),
        "the exhausted event carries the recorded count: {exhausted:?}"
    );
}

/// A permanent refusal is terminal on the first try and still counts once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_permanent_refusal_counts_one_cycle_and_is_acknowledged() {
    let _capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000002-2";
    let entry = owe_and_queue(&harness, 6, event).await;
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Permanent]);
    let lanes = lanes_over(&server, harness.database.clone(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until("the refused job to be acknowledged", || {
        server.acks().contains(&entry.as_str().to_owned())
    })
    .await;
    token.cancel();
    lanes.drain().await;

    assert_eq!(poster.calls(), 1, "a permanent verdict is not retried");
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 1);
    assert_eq!(delivered_at, None);
}

/// A shutdown mid-retry hands the entry back and keeps the cycle it counted.
///
/// The token is cancelled while the poster is still refusing, so `when` stops
/// the retries and the verdict comes back `Retryable` with the token cancelled.
/// That branch acknowledges NOTHING: the entry stays in this consumer's pending
/// list for the next process. The count already recorded stands — the next
/// process's cycle will count a second one, which is true.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn shutdown_requeue_preserves_the_pending_entry() {
    let capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000002-3";
    let entry = owe_and_queue(&harness, 7, event).await;
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Retryable]);
    let lanes = lanes_over(&server, harness.database.clone(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    // Cancel as soon as the first refusal has been given: the retry loop reads
    // the token before its next attempt and stops there.
    await_until("the poster to be asked once", || poster.calls() >= 1).await;
    token.cancel();
    lanes.drain().await;

    assert!(
        server.acks().is_empty(),
        "a job handed back at shutdown is not acknowledged: {:?}",
        server.acks()
    );
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 1, "the cycle that was cut short still started");
    assert_eq!(delivered_at, None);
    let requeued = capture.named(EVENT_REQUEUED);
    assert!(
        requeued.iter().any(|seen| seen.attempts == Some(1)),
        "the requeue event carries the recorded count: {requeued:?}"
    );
}

/// A ledger that will not answer costs the count, never the answer.
///
/// Both bookkeeping writes fail — the cycle start and the success stamp — and
/// the answer is still delivered and still acknowledged. The count is not
/// recorded, which the delivered event says by carrying no count rather than a
/// wrong one, and both failures are reported by name.
#[tokio::test(flavor = "multi_thread")]
async fn bookkeeping_failure_does_not_discard_an_answer() {
    let capture = capture();
    let event = "1700000003-0";
    let entry = EventId::of("1700000003000-0");
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Delivered]);
    let lanes = lanes_over(&server, no_ledger::no_ledger(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until(
        "the answer to be acknowledged despite the dead ledger",
        || server.acks().contains(&entry.as_str().to_owned()),
    )
    .await;
    token.cancel();
    lanes.drain().await;

    assert_eq!(poster.calls(), 1, "the destination was given the answer");
    assert!(
        !capture.named(EVENT_COUNT_FAILED).is_empty(),
        "the cycle-start failure is reported: {:?}",
        capture.events()
    );
    assert!(
        !capture.named(EVENT_STAMP_FAILED).is_empty(),
        "the stamp failure is reported: {:?}",
        capture.events()
    );
    assert!(
        capture.named(EVENT_DELIVERED).is_empty(),
        "with the stamp refused, no delivered event claims a count"
    );
}

// ── Abandonment ─────────────────────────────────────────────────────────────

/// The instant every scan below is asked from: past any row this suite wrote,
/// so a row the scans return is one they would offer at any cutoff.
const FAR_FUTURE: i64 = i64::MAX / 2;

/// The row's abandonment, as the ledger holds it: `(abandoned_at, reason)`.
async fn abandonment(harness: &OutboundHarness, event: &str) -> (Option<i64>, Option<String>) {
    let mut connection = harness
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    sqlx::query_as(
        "SELECT abandoned_at, abandon_reason FROM core.fleet_obligations
          WHERE fleet_id = $1::uuid AND event_id = $2::text",
    )
    .bind(FLEET)
    .bind(event)
    .fetch_one(&mut *connection)
    .await
    .expect("reading the obligation")
}

/// Every event either recovery scan would offer, at any cutoff.
async fn offered(harness: &OutboundHarness) -> Vec<String> {
    let cutoff = UnixMillis::from_millis(FAR_FUTURE);
    let unreceipted = obligation::unreceipted(&harness.database, cutoff, AMPLE)
        .await
        .expect("the unreceipted scan answers");
    let undelivered = obligation::undelivered(&harness.database, cutoff, AMPLE)
        .await
        .expect("the undelivered scan answers");
    unreceipted
        .into_iter()
        .chain(undelivered)
        .map(|owed| owed.event_id)
        .collect()
}

/// Runs one delivery cycle of `event` through lanes whose poster answers
/// `verdict`, and waits for its acknowledgement.
async fn one_cycle(harness: &OutboundHarness, entry: EventId, event: &str, verdict: Verdict) {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[verdict]);
    let lanes = lanes_over(&server, harness.database.clone(), poster, &token).await;
    lanes.dispatch(job(entry.clone(), event)).await;
    await_until("the cycle's job to be acknowledged", || {
        server.acks().contains(&entry.as_str().to_owned())
    })
    .await;
    token.cancel();
    lanes.drain().await;
}

/// Dimension 4.1 — a permanent refusal abandons the row, the abandonment is
/// announced once, and no scan offers the answer again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn permanent_refusal_abandons_the_obligation() {
    let capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000004-0";
    let entry = owe_and_queue(&harness, 20, event).await;
    let announced_before = capture.named(EVENT_ABANDONED).len();

    one_cycle(&harness, entry, event, Verdict::Permanent).await;

    let (abandoned_at, reason) = abandonment(&harness, event).await;
    assert!(abandoned_at.is_some(), "a refusal for good is abandoned");
    assert_eq!(reason.as_deref(), Some(AbandonReason::Refused.as_str()));
    assert!(
        !offered(&harness).await.contains(&event.to_owned()),
        "an abandoned answer is never re-offered, at any cutoff"
    );
    assert_eq!(
        capture.named(EVENT_ABANDONED).len(),
        announced_before + 1,
        "the abandonment is announced once"
    );
    assert_eq!(
        obligation::abandon(
            &harness.database,
            FLEET,
            event,
            AbandonReason::Refused,
            UnixMillis::from_millis(SEEDED_AT + 50),
        )
        .await
        .expect("the ledger answers"),
        None,
        "a second abandon stamps nothing, so nothing is announced twice"
    );
}

/// Dimension 4.2 — a destination failing retryably is re-offered while cycles
/// remain, and abandoned on the cycle that reaches the cap.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn exhausted_cycles_abandon_the_obligation() {
    let _capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000004-1";
    let entry = owe_and_queue(&harness, 21, event).await;
    // Every cycle but the last two already spent, as the ledger would hold it
    // after that many re-offers.
    set_attempts(&harness, event, MAX_DELIVERY_CYCLES - 2).await;

    one_cycle(&harness, entry, event, Verdict::Retryable).await;
    assert_eq!(
        abandonment(&harness, event).await,
        (None, None),
        "one cycle is still left, so the answer is still owed"
    );
    assert!(
        offered(&harness).await.contains(&event.to_owned()),
        "and the scan still offers it"
    );

    one_cycle(
        &harness,
        EventId::of("1700000004999-1"),
        event,
        Verdict::Retryable,
    )
    .await;
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(
        attempts, MAX_DELIVERY_CYCLES,
        "the capping cycle was counted"
    );
    assert_eq!(delivered_at, None);
    let (abandoned_at, reason) = abandonment(&harness, event).await;
    assert!(
        abandoned_at.is_some(),
        "the cycle that reached the cap abandoned it"
    );
    assert_eq!(
        reason.as_deref(),
        Some(AbandonReason::CyclesExhausted.as_str())
    );
    assert!(
        !offered(&harness).await.contains(&event.to_owned()),
        "an answer out of cycles is never re-offered"
    );
}

/// Dimension 4.3 — rows written before an obligation had to name a
/// destination are offered by neither scan, receipted or not, at any cutoff.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn legacy_rows_are_never_reoffered() {
    let _capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let unqueued = "1700000004-2";
    let queued = "1700000004-3";
    seed_legacy(&harness, 22, unqueued, None).await;
    seed_legacy(&harness, 23, queued, Some("1700000000000-9")).await;

    let offered = offered(&harness).await;
    for event in [unqueued, queued] {
        assert!(
            !offered.contains(&event.to_owned()),
            "{event} names no destination and must never be offered: {offered:?}"
        );
    }
}

/// Sets the row's spent delivery cycles, as that many re-offers would leave it.
async fn set_attempts(harness: &OutboundHarness, event: &str, cycles: i64) {
    let mut connection = harness
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    sqlx::query(
        "UPDATE core.fleet_obligations SET attempt_count = $3
          WHERE fleet_id = $1::uuid AND event_id = $2::text",
    )
    .bind(FLEET)
    .bind(event)
    .bind(cycles)
    .execute(&mut *connection)
    .await
    .expect("seeding the spent cycles");
}

/// Writes a row the way the report path did before it read a destination:
/// owed to the model provider, naming nowhere.
async fn seed_legacy(harness: &OutboundHarness, nth: u8, event: &str, receipt: Option<&str>) {
    let mut connection = harness
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    sqlx::query(
        "INSERT INTO core.fleet_obligations
           (id, fleet_id, workspace_id, provider, event_id, answer, receipt,
            attempt_count, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, 0, $8, $8)",
    )
    .bind(obligation_id(nth))
    .bind(FLEET)
    .bind(WORKSPACE)
    .bind(LEGACY_PROVIDER)
    .bind(event)
    .bind(ANSWER)
    .bind(receipt)
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a row the old report path wrote");
}

/// What the report path wrote into `provider` before it read a destination:
/// the lease's model provider.
const LEGACY_PROVIDER: &str = "anthropic";
