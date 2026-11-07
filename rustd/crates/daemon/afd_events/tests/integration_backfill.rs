//! Dimension 5.4 — the reconnect gap, and the overlap that closes it.
//!
//! A client watching a fleet holds two sources of the same narrative log: the
//! live tail, which is pub/sub and keeps nothing, and the events list, which is
//! Postgres and keeps everything. A dropped connection is the moment those two
//! have to be reconciled, and the documented recipe is one line — refetch from
//! **two seconds before the last frame you were delivered**, and merge by event
//! id.
//!
//! # What the two seconds are for, and why the merge is not optional
//!
//! An event becomes visible on the two paths at different moments: the frame is
//! published from the process that wrote it, and the row becomes readable when
//! its transaction commits. Refetching from exactly the last delivery would
//! therefore race — an event published just before the drop whose row committed
//! just after would fall through the seam and be in neither set. Backing the
//! window up guarantees the two sets OVERLAP instead.
//!
//! An overlap is only safe if the client can tell one event seen twice from two
//! events, and that is what the event id is: the stream entry id, carried
//! identically on the live frame and on the row (dimension 5.3 pins that they
//! are the same value). So the price of never missing an event is receiving a
//! few twice, and the merge is what the client pays it with.
//!
//! # What is proven here, and what belongs to the client
//!
//! The SERVER halves are proven: that the window is an inclusive lower bound on
//! `created_at`, that it returns the gap rows, and that it deliberately returns
//! rows the client already has. The merge itself is the client's arithmetic, and
//! it is performed here over the two REAL sets — frames actually delivered
//! through a live hub, rows actually read back through `History` — because the
//! claim worth making is about those two sets together. A merge run over two
//! hand-written vectors would prove only that `HashSet` deduplicates.
//!
//! Marked `#[ignore]` so `make test-unit-all` still COMPILES and lints this
//! without datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes it.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_dragonfly::SubscriptionHub;
use afd_dragonfly::fleet_activity_channel;
use afd_dragonfly::streams::FleetStreams;
use afd_events::{Filter, History, MAX_LIMIT, Since};
use afd_sse::tail;
use futures_util::StreamExt as _;
use jiff::Timestamp;

use crate::support::{DELIVERY_BUDGET, EventsLane};

/// The instant the fixture calls "now".
///
/// Fixed rather than read from a clock: every assertion below is arithmetic on
/// row timestamps, and a fixture that could not name the instant could not
/// state the window it expects.
const NOW: Timestamp = Timestamp::constant(1_800_000_000, 0);

/// The overlap the documented reconnect recipe backs the window up by.
///
/// Two seconds, and named once here rather than spelled at each use: the whole
/// dimension is a claim about THIS number.
const OVERLAP: Duration = Duration::from_secs(2);

/// How long before the drop the client's LAST delivered event was written.
///
/// Inside the overlap on purpose: this is the event the reconnect window must
/// re-serve, and the merge must then remove.
const LAST_DELIVERY_AGO: Duration = Duration::from_secs(1);

/// How long before the drop the client's SECOND event was written.
const SECOND_DELIVERY_AGO: Duration = Duration::from_secs(5);

/// How long before the drop the client's FIRST event was written.
///
/// Outside the overlap, which is what makes the lower-bound assertion mean
/// something: a window that reached this far back would be resending history a
/// long-lived client already holds.
const FIRST_DELIVERY_AGO: Duration = Duration::from_secs(10);

/// How long after the drop the first gap event lands.
const FIRST_GAP_AFTER: Duration = Duration::from_millis(500);

/// How long after the drop the second gap event lands.
const SECOND_GAP_AFTER: Duration = Duration::from_secs(1);

/// `NOW` moved by `span`, earlier or later.
fn before_now(span: Duration) -> Timestamp {
    NOW.checked_sub(span)
        .expect("a fixture instant is in range")
}
fn after_now(span: Duration) -> Timestamp {
    NOW.checked_add(span)
        .expect("a fixture instant is in range")
}

/// Dimension 5.4 — a reconnect misses nothing, and the client sees each event
/// once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_sse_reconnect_backfill() {
    let lane = EventsLane::open().await;

    // Five events across the drop. An admitted event is named by its
    // admission row's UUIDv7, so the fixture mints ids of that shape rather
    // than inventing one this daemon never serves.
    let delivered = [mint_id(), mint_id(), mint_id()];
    let gap = [mint_id(), mint_id()];
    let delivered_ids = delivered.each_ref().map(String::as_str);
    let gap_ids = gap.each_ref().map(String::as_str);

    lane.seed_event(delivered_ids[0], before_now(FIRST_DELIVERY_AGO))
        .await;
    lane.seed_event(delivered_ids[1], before_now(SECOND_DELIVERY_AGO))
        .await;
    lane.seed_event(delivered_ids[2], before_now(LAST_DELIVERY_AGO))
        .await;

    // The live half, over a real hub. These three are what the connection was
    // handed before it dropped; the last of them is the delivery the client
    // measures its reconnect window from.
    let live = deliver_live(&lane, &delivered_ids).await;
    assert_eq!(
        live.len(),
        delivered_ids.len(),
        "the fixture must actually have delivered the frames it claims"
    );

    // The drop. These two rows land while nobody is subscribed, so pub/sub
    // never offers them to anyone — recovering them is the whole job.
    lane.seed_event(gap_ids[0], after_now(FIRST_GAP_AFTER))
        .await;
    lane.seed_event(gap_ids[1], after_now(SECOND_GAP_AFTER))
        .await;

    // The reconnect. The client backs its window up from its LAST DELIVERY —
    // not from "now", which it has no reason to trust, and not from the first
    // frame of the connection, which would refetch the whole session.
    let last_delivery = before_now(LAST_DELIVERY_AGO);
    let since = last_delivery
        .checked_sub(OVERLAP)
        .expect("a fixture instant is in range");
    let backfill = page_since(&lane, Since::From(since)).await;

    assert_window(&backfill, &delivered_ids, &gap_ids);
    assert_merged(&live, &backfill, &delivered_ids, &gap_ids);

    lane.cleanup().await;
}

/// The window is an INCLUSIVE lower bound, and it is doing both of its jobs:
/// it reaches back far enough to re-serve an event the client already has, and
/// forward far enough to carry the two it does not.
fn assert_window(backfill: &BTreeSet<String>, delivered: &[&str; 3], gap: &[&str; 2]) {
    assert!(
        backfill.contains(delivered[2]),
        "the two-second overlap must re-serve the last delivered event; a \
         window that started exactly at the last delivery would race the \
         commit of an event published just before the drop"
    );
    assert!(
        backfill.contains(gap[0]) && backfill.contains(gap[1]),
        "every event written during the gap must be in the backfill"
    );
    assert!(
        !backfill.contains(delivered[0]) && !backfill.contains(delivered[1]),
        "the window is bounded below: events older than it must not be resent, \
         or a long-lived client would refetch its whole history on every blip"
    );
}

/// The merge — the client's half, over the two sets it really holds.
fn assert_merged(
    live: &[String],
    backfill: &BTreeSet<String>,
    delivered: &[&str; 3],
    gap: &[&str; 2],
) {
    let mut merged: BTreeSet<String> = live.iter().cloned().collect();
    let before_merge = merged.len() + backfill.len();
    merged.extend(backfill.iter().cloned());
    assert!(
        before_merge > merged.len(),
        "the two sets must OVERLAP: an overlap the merge has nothing to remove \
         would mean the window never reached back over the seam"
    );
    let expected: BTreeSet<String> = delivered
        .iter()
        .chain(gap.iter())
        .map(|id| (*id).to_owned())
        .collect();
    assert_eq!(
        merged, expected,
        "merged by event id, the client holds every event exactly once — \
         gapless across the drop and duplicate-free across the overlap"
    );
}

/// The event ids the client is delivered live, through a real hub.
///
/// Publishes each event's frame and reads it back off the tail, so the returned
/// set is what a connection ACTUALLY received rather than what the fixture
/// intended it to.
async fn deliver_live(lane: &EventsLane, ids: &[&str]) -> Vec<String> {
    let publisher = FleetStreams::new(lane.queue.clone());
    let activity = fleet_activity_channel(&lane.fleet);
    let hub = SubscriptionHub::start(EventsLane::redis())
        .await
        .expect("the hub starts");

    // A primer, so the server-side SUBSCRIBE is known live before anything the
    // test counts is published. `subscribe` queues the command and the pump
    // issues it, so the registration is asynchronous by construction.
    let mut primer = hub.subscribe(&activity);
    let marker = r#"{"kind":"primer"}"#;
    let deadline = tokio::time::Instant::now() + DELIVERY_BUDGET;
    loop {
        publisher
            .publish(&activity, marker)
            .await
            .expect("the publish reaches Dragonfly");
        match tokio::time::timeout(std::time::Duration::from_millis(100), primer.recv()).await {
            Ok(Ok(afd_dragonfly::hub::Received::Message(message))) if message.payload == marker => {
                break;
            }
            Ok(Ok(_other)) => {}
            Ok(Err(closed)) => panic!("the hub closed while priming: {closed}"),
            Err(_elapsed) => {}
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the subscription never went live on {activity}"
        );
    }

    let mut stream = Box::pin(tail(hub.subscribe(&activity)));
    for id in ids {
        publisher
            .publish(&activity, &activity_payload(id))
            .await
            .expect("the publish reaches Dragonfly");
    }

    let mut received = Vec::with_capacity(ids.len());
    for _ in ids {
        let frame = tokio::time::timeout(DELIVERY_BUDGET, stream.next())
            .await
            .expect("a frame must arrive inside the delivery budget")
            .expect("the tail must not end while a frame is owed");
        received.push(event_id_of(&frame.data.text()));
    }
    received
}

/// One activity frame's payload, as a producer publishes one.
///
/// `kind` leads because that is the anchor the frame's `event:` name is read
/// from, and `event_id` carries the value the merge is performed on — the same
/// stream entry id the row is stored under.
fn activity_payload(event_id: &str) -> String {
    format!(r#"{{"kind":"run_output","event_id":"{event_id}"}}"#)
}

/// The `event_id` a delivered frame carries.
///
/// Parsed rather than assumed: the frame's data is the publisher's bytes
/// forwarded unrewritten, and reading the id back out is precisely what a
/// client does before merging.
fn event_id_of(data: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(data).expect("a delivered frame carries JSON");
    parsed
        .get("event_id")
        .and_then(serde_json::Value::as_str)
        .expect("every activity frame names the event it belongs to")
        .to_owned()
}

/// A relative window is measured back from the DATABASE's clock: rows an hour
/// old fall outside a thirty-minute window and a row a minute old falls inside
/// it, with no instant computed in Rust. The rows are stamped from this host's
/// clock, which shares a machine with the lane's Postgres; the margins are
/// minutes, so no skew between the two decides the outcome.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_since_span_is_measured_on_the_database_clock() {
    let lane = EventsLane::open().await;
    let (old, recent) = (mint_id(), mint_id());
    let now = Timestamp::now();
    let ago = |span: Duration| now.checked_sub(span).expect("an instant in range");
    lane.seed_event(&old, ago(Duration::from_hours(1))).await;
    lane.seed_event(&recent, ago(Duration::from_mins(1))).await;

    let window = page_since(&lane, Since::Within(Duration::from_mins(30))).await;
    assert!(
        window.contains(&recent),
        "a minute-old row is inside the window"
    );
    assert!(!window.contains(&old), "an hour-old row is outside it");

    lane.cleanup().await;
}

/// The event ids the history returns for this fleet from `since` onward.
async fn page_since(lane: &EventsLane, since: Since) -> BTreeSet<String> {
    let history = History::new(lane.database.clone());
    let workspace = Uuid7::parse(&lane.workspace).expect("the fixture workspace id is canonical");
    let fleet = Uuid7::parse(&lane.fleet).expect("the fixture fleet id is canonical");
    let filter = Filter {
        actor_like: None,
        since: Some(since),
    };

    history
        .page_for_fleet(&workspace, &fleet, &filter, None, MAX_LIMIT)
        .await
        .expect("the history read must run")
        .into_iter()
        .map(|row| row.event_id)
        .collect()
}
