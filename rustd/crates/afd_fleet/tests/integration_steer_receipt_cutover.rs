//! A person's message that failed between its row and its receipt is still
//! one message: one resend, one sweep, one lease.
//!
//! `afd_admission`'s receipt suite proves each step apart, and
//! `integration_admission_recovery` proves both crash points for a webhook.
//! Neither shows the whole timeline for a steer, the one producer a person
//! watches in the thread while it waits: the resend under the same operation
//! id, the page that lists it meanwhile, the sweep that owes it a receipt, and
//! the lease that runs it. Each test here crosses all four and reads the
//! ledger, the stream, the thread and the event table back after each.
//!
//! The failure points are produced the way the recovery suite produces them:
//! a queue nothing listens on for "before the append", and that plus the entry
//! the dead inserter had already written for "after the append, before the
//! receipt". The second physical copy a sweep leaves is driven through the
//! same `record_received` verb the lease path runs for every copy it reads.
//!
//! Serialised on [`RECOVERY_LANE`], because the replay pass is
//! deployment-wide. Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock;
use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_dragonfly::{EventId, FleetStreams};
use afd_events::{Cursor, History, Steer};
use afd_fleet::lease::{Acquired, Delivery};
use afd_wire::event::{Entry, EventType};

use crate::integration_admission_recovery::{
    EVERY_ROW, NO_GRACE, RECOVERY_LANE, deferring, ledger,
};
use crate::queue;
use crate::rows::Row;
use crate::seed::{seeded_parts, select_fleet_within_rotations};
use crate::support::Fixtures;

/// A person's actor, as the steer route records it.
const ACTOR: &str = "steer:user_receipt";

/// What they typed, as the route stores it.
const BODY: &str = r#"{"message":"restart the canary"}"#;

/// The operation id the client repeats when it resends.
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0c71";

/// A page of one, so a repeat of the message would have to cross a cursor.
const PAGE: i64 = 1;

/// One `fleet.events` row per logical event, whatever the stream held.
const COUNT_EVENT_ROWS: &str =
    "SELECT COUNT(*) FROM fleet.events WHERE fleet_id = $1::uuid AND event_id = $2";

/// Where the send stopped.
#[derive(Clone, Copy, Debug)]
enum Failure {
    /// The row committed and the queue never took the entry.
    BeforeAppend,
    /// The queue took the entry and the receipt was never recorded.
    AfterAppend,
}

impl Failure {
    /// Physical entries the stream holds for the message before the sweep.
    const fn copies_before_sweep(self) -> usize {
        match self {
            Self::BeforeAppend => 0,
            Self::AfterAppend => 1,
        }
    }

    /// Physical entries once the sweep re-appended it.
    const fn copies_after_sweep(self) -> usize {
        self.copies_before_sweep() + 1
    }
}

/// After the append and before the receipt: the resend appends nothing, the
/// thread lists one waiting message, the sweep adds the second copy, and the
/// lease opens one event however many copies it reads.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_cutover_preserves_runtime_and_steer_receipt() {
    let _lane = RECOVERY_LANE.lock().await;
    one_message_across_resend_sweep_and_lease(Failure::AfterAppend).await;
}

/// Before the append: the same timeline, with the sweep's entry the only copy.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_message_whose_append_never_landed_is_one_event_after_the_sweep() {
    let _lane = RECOVERY_LANE.lock().await;
    one_message_across_resend_sweep_and_lease(Failure::BeforeAppend).await;
}

async fn one_message_across_resend_sweep_and_lease(failure: Failure) {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let streams = FleetStreams::new(fixtures.queue().clone());

    // ── The send: its row commits through a queue that is not there.
    let sent = Steer::new(deferring(&fixtures))
        .append(&fleet, &workspace, ACTOR, BODY, Some(OPERATION))
        .await
        .expect("the row commits whatever the queue does");
    assert!(!sent.replayed(), "the first send admits the message");
    let event_id = sent.event_id;
    if matches!(failure, Failure::AfterAppend) {
        append_as_the_dead_inserter(&streams, &fleet, &workspace, &event_id).await;
    }
    assert_eq!(
        fixtures.text(Row::Admission(&event_id), "receipt").await,
        None,
        "{failure:?}: no receipt was recorded"
    );
    assert_eq!(
        copies(&fixtures, &fleet, &event_id).await,
        failure.copies_before_sweep()
    );

    // ── The resend, under the same operation id, once the queue is back.
    let resent = Steer::new(ledger(&fixtures))
        .append(&fleet, &workspace, ACTOR, BODY, Some(OPERATION))
        .await
        .expect("a resend is answered");
    assert!(resent.replayed(), "the resend answers the first send");
    assert_eq!(resent.event_id, event_id, "with the first send's event");
    assert_eq!(
        fixtures.admissions_for(&fleet).await,
        1,
        "one operation id is one ledger row"
    );
    assert_eq!(
        copies(&fixtures, &fleet, &event_id).await,
        failure.copies_before_sweep(),
        "{failure:?}: the resend appends nothing"
    );

    // ── While it waits, the thread lists it once.
    let thread = Thread::of(&fixtures, &workspace, &fleet);
    thread.assert_only(&event_id, status::QUEUED).await;

    // ── The sweep owes it a receipt and re-appends it once.
    ledger(&fixtures)
        .replay(NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs against both live datastores");
    let receipt = fixtures
        .text(Row::Admission(&event_id), "receipt")
        .await
        .expect("the sweep records a receipt");
    assert_eq!(
        fixtures
            .value::<i64>(Row::Admission(&event_id), "replay_count")
            .await
            .expect("replay_count is NOT NULL"),
        1,
        "re-appended exactly once"
    );
    assert!(
        streams
            .holds_entry(&fleet, &EventId::of(&receipt))
            .await
            .expect("the stream answers"),
        "the receipt names an entry the stream holds"
    );
    assert_eq!(
        copies(&fixtures, &fleet, &event_id).await,
        failure.copies_after_sweep(),
        "{failure:?}: the sweep's copy beside any the dead inserter left"
    );
    thread.assert_only(&event_id, status::QUEUED).await;

    // ── The lease: every copy the stream holds is the same event.
    lease_every_copy(&fixtures, &fleet, &runner, &event_id).await;
    assert_eq!(
        event_rows(&fixtures, &fleet, &event_id).await,
        1,
        "{failure:?}: one event row"
    );
    thread.assert_only(&event_id, status::RECEIVED).await;

    streams.forget(&fleet).await.expect("purging the stream");
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// Leases the message once, then drives every other copy the sweep left
/// through the same verb: each is the open event again, never a second one.
async fn lease_every_copy(fixtures: &Fixtures, fleet: &str, runner: &Uuid7, event_id: &str) {
    let leases = fixtures.leases();
    let acquired = select_fleet_within_rotations(&leases, runner, fleet)
        .await
        .expect("the fleet holding the message is leasable");
    assert_eq!(acquired.event_id, event_id);
    let opened = leases
        .record_received(&acquired)
        .await
        .expect("the narrative log opens");
    assert_eq!(opened.delivery, Delivery::First);
    for copy in other_copies(fixtures, fleet, &acquired).await {
        let again = Acquired {
            receipt: EventId::of(&copy),
            ..acquired.clone()
        };
        let repeat = leases
            .record_received(&again)
            .await
            .expect("a second copy must not fault");
        assert_eq!(
            repeat.delivery,
            Delivery::Repeat,
            "a second copy of an open event is that event, not another"
        );
        assert_eq!(repeat.counters, None, "and it moves no counter");
    }
}

/// Appends the entry a crashed inserter had already put on the stream: the
/// fields `admit` writes for a steer, under the message's logical id.
async fn append_as_the_dead_inserter(
    streams: &FleetStreams,
    fleet: &str,
    workspace: &str,
    event_id: &str,
) {
    let created_at = clock::now().to_string();
    let entry = Entry {
        actor: ACTOR,
        event_type: EventType::Chat.as_str(),
        workspace_id: workspace,
        request_json: BODY,
        created_at: &created_at,
    };
    streams
        .append(fleet, &entry.queued_pairs(event_id))
        .await
        .expect("the lane's stream takes an append");
}

/// How many physical entries carry `event_id`.
async fn copies(fixtures: &Fixtures, fleet: &str, event_id: &str) -> usize {
    queue::entries_on(fixtures.queue(), fleet)
        .await
        .iter()
        .filter(|(_receipt, logical)| logical == event_id)
        .count()
}

/// Every copy of the acquired event except the one the lease arrived on.
async fn other_copies(fixtures: &Fixtures, fleet: &str, acquired: &Acquired) -> Vec<String> {
    queue::entries_on(fixtures.queue(), fleet)
        .await
        .into_iter()
        .filter(|(copy, logical)| {
            *logical == acquired.event_id && copy.as_str() != acquired.receipt.as_str()
        })
        .map(|(copy, _logical)| copy)
        .collect()
}

async fn event_rows(fixtures: &Fixtures, fleet: &str, event_id: &str) -> i64 {
    sqlx::query_scalar(COUNT_EVENT_ROWS)
        .bind(fleet)
        .bind(event_id)
        .fetch_one(
            &mut *fixtures
                .database
                .acquire()
                .await
                .expect("a pooled connection"),
        )
        .await
        .expect("the event table answers a count")
}

/// One fleet's thread, read a page of one at a time.
struct Thread {
    history: History,
    workspace: Uuid7,
    fleet: Uuid7,
}

impl Thread {
    fn of(fixtures: &Fixtures, workspace: &str, fleet: &str) -> Self {
        let uuid = |text: &str| Uuid7::parse(text).expect("the fixture mints canonical ids");
        Self {
            history: History::new(fixtures.database.clone()),
            workspace: uuid(workspace),
            fleet: uuid(fleet),
        }
    }

    /// The thread holds `event_id` once, as `status`, and nothing past it.
    async fn assert_only(&self, event_id: &str, status: &str) {
        let first = self
            .history
            .thread_page(&self.workspace, &self.fleet, None, PAGE)
            .await
            .expect("the thread reads");
        let listed: Vec<_> = first
            .iter()
            .map(|row| (row.row.event_id.as_str(), row.row.status.as_str()))
            .collect();
        assert_eq!(listed, [(event_id, status)], "one row, as {status}");
        let last = first.last().expect("the page holds the message");
        let cursor = Cursor::after(last.row.created_at, &last.row.event_id);
        let rest = self
            .history
            .thread_page(&self.workspace, &self.fleet, Some(&cursor), PAGE)
            .await
            .expect("the resumed thread reads");
        assert!(
            rest.is_empty(),
            "the message is not listed again past its own cursor: {rest:?}"
        );
    }
}
