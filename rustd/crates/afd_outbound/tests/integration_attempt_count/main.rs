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
//! directly against the shared schema, the way `integration_obligations/main.rs`
//! does. The tests that grade the WORKER drive `Lanes` with a scripted poster,
//! the way `lanes/main.rs` does, and take their ledger from the harness so the
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
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::streams::EventId;
use afd_dragonfly::{Dragonfly, OutboundDelivery, OutboundJob, OutboundQueue};
use afd_outbound::obligation::{self, AbandonReason, Delivery};
use afd_outbound::producer::{LOST_AFTER, MAX_DELIVERY_CYCLES};
use afd_outbound::retry::DELIVERY_ATTEMPTS;
use afd_outbound::{Attempt, Deliver, Lanes, Posters, Verdict};
use tokio_util::sync::CancellationToken;
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt as _};

#[path = "../support/hanging_queue.rs"]
#[allow(
    clippy::expect_used,
    dead_code,
    reason = "shared support: this suite needs the ack log, not the read counter"
)]
mod hanging_queue;
#[path = "../support/no_ledger.rs"]
mod no_ledger;
#[path = "../support/obligation_seed.rs"]
#[allow(
    dead_code,
    reason = "shared seed: this suite owes and counts, it does not forget streams"
)]
mod seed;
#[path = "../support/outbound_harness.rs"]
#[allow(
    dead_code,
    reason = "one harness, several binaries: this one drives the ledger and the lanes"
)]
mod support;

use self::hanging_queue::HangingQueue;
use self::seed::{
    DESTINATION, FLEET, WORKSPACE, abandonment, age, clear_obligations, obligation_id, seed_parents,
};
use self::support::{OUTBOUND_LANE, OutboundHarness};

/// The connector every fixture answer goes back through.
const PROVIDER: &str = Provider::Slack.id();

/// What the fixture answers say.
const ANSWER: &str = "Aurora is healthy.";
/// More rows than any test seeds, so a scan's limit never decides an assertion.
const AMPLE: i64 = 64;
/// How long a lane is given to reach a terminal verdict and acknowledge.
///
/// Three attempts with two jittered sleeps come to under a second and a half;
/// the rest is the coverage lane's oversubscription, which `lanes/main.rs` measured
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
const EVENT_ABANDON_FAILED: &str = "outbound_obligation_abandon_failed";
/// The field the count rides on.
const FIELD_ATTEMPTS: &str = "attempts";
/// The field an event is named by.
const FIELD_EVENT: &str = "event";
/// The field naming the turn an event belongs to. One global capture serves
/// every test in this binary, so an assertion that nothing happened has to say
/// nothing happened TO ITS OWN TURN — a sibling's delivery is not this one's.
const FIELD_EVENT_ID: &str = "agentsfleet_event_id";

// ── The ledger half ─────────────────────────────────────────────────────────

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

    let (_, _, aged) = row(&harness, event).await;
    let first = obligation::count_attempt(&harness.database, FLEET, event)
        .await
        .expect("counting the first cycle");
    let second = obligation::count_attempt(&harness.database, FLEET, event)
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
    assert!(
        updated_at > aged,
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

    let counted = obligation::count_attempt(&harness.database, FLEET, event)
        .await
        .expect("counting the cycle that delivers");
    assert_eq!(counted, Some(1));
    obligation::stamp_delivered(&harness.database, FLEET, event)
        .await
        .expect("stamping the delivery");
    let (_, accepted_at, _) = row(&harness, event).await;

    let again = obligation::count_attempt(&harness.database, FLEET, event)
        .await
        .expect("a duplicate cycle asks and is refused");
    obligation::stamp_delivered(&harness.database, FLEET, event)
        .await
        .expect("a duplicate stamp asks and is refused");

    assert_eq!(again, None, "an already-delivered row counts no cycle");
    let (attempts, delivered_at, updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 1, "the counter did not move for the duplicate");
    assert!(accepted_at.is_some(), "the delivery was stamped");
    assert_eq!(delivered_at, accepted_at, "the first acceptance stands");
    assert_eq!(
        Some(updated_at),
        accepted_at,
        "nothing after the stamp touched the row"
    );
}

/// A cycle start takes the row out of the recovery scan's window.
///
/// The pacing decision, proven in both directions. `SELECT_UNDELIVERED` asks
/// for rows untouched for [`LOST_AFTER`] on the database clock. A row a worker
/// just accepted is being worked on, so the scan does not re-offer it; once
/// that long passes with nobody touching it — which `seed::age` stands in for —
/// it does, which is the case of a worker that died mid-cycle and the scan
/// exists for.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn recovery_pacing_follows_the_cycle_start() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000001-2";
    owe_and_queue(&harness, 3, event).await;

    assert_eq!(
        reoffered(&harness).await,
        vec![event.to_owned()],
        "before any cycle starts, the queued row is the scan's business"
    );
    obligation::count_attempt(&harness.database, FLEET, event)
        .await
        .expect("counting the cycle");

    assert!(
        reoffered(&harness).await.is_empty(),
        "a row whose cycle just started is somebody's work, not the scan's"
    );
    age(&harness.database, event).await;
    assert_eq!(
        reoffered(&harness).await,
        vec![event.to_owned()],
        "once the window passes the start, a cycle that never finished is re-offered"
    );
}

mod abandonment;
mod capture;
mod ledger;
mod worker;

use self::ledger::*;
