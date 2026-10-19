//! Dimension 4.9 — an admitted event is named by its `fleet.admissions` row id,
//! and the history keyset `(created_at, event_id)` reads two such events in one
//! millisecond newest-first, each once, across a page boundary. A stream-style
//! `{millis}-{seq}` id names nothing.
//!
//! Lives here rather than beside the receipts in `afd_admission`, because the
//! history read is this crate's and `afd_admission` cannot depend on it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Key, Producer};
use afd_core::id::Uuid7;
use afd_dragonfly::streams::FleetStreams;
use afd_events::{Filter, History, next_cursor};
use afd_wire::event::EventType;
use jiff::Timestamp;

use crate::support::EventsLane;

/// The one millisecond both events are recorded at.
const SHARED_INSTANT: Timestamp = Timestamp::constant(1_800_000_000, 7_000_000);

/// A legacy stream-entry id, the shape event ids had before the row id.
const LEGACY_ID: &str = "1800000000007-0";

fn admission<'a>(lane: &'a EventsLane, key: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Steer,
        key: Key::Repeated(key),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: "steer:api",
        event_type: EventType::Chat,
        request_json: r#"{"message":"history fixture"}"#,
        reply: afd_admission::Reply::None,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_admissions_in_one_millisecond_list_once_newest_first() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("group create");
    let ledger = lane.admissions();
    let mut ids = Vec::new();
    for name in ["first", "second"] {
        let key = format!("{}-{name}", lane.fleet);
        let admitted = ledger
            .admit(admission(&lane, &key))
            .await
            .expect("the admission is taken");
        Uuid7::parse(&admitted.stored.id).expect("an event id is the row's UUIDv7");
        // Delivered as the worker records it: under the admission's own id,
        // both at one instant so only the id can break the tie.
        lane.seed_event(&admitted.stored.id, SHARED_INSTANT).await;
        ids.push(admitted.stored.id);
    }
    let mut newest_first = ids.clone();
    newest_first.sort_by(|older, newer| newer.cmp(older));

    let history = History::new(lane.database.clone());
    let workspace = Uuid7::parse(&lane.workspace).expect("the fixture workspace parses");
    let fleet = Uuid7::parse(&lane.fleet).expect("the fixture fleet parses");
    let filter = Filter::default();

    let whole = history
        .page_for_fleet(&workspace, &fleet, &filter, None, 10)
        .await
        .expect("a page reads");
    let listed: Vec<_> = whole.iter().map(|row| row.event_id.clone()).collect();
    assert_eq!(listed, newest_first);

    // One row a page: the cursor pair carries the tie across the boundary.
    let mut walked = Vec::new();
    let mut cursor = None;
    loop {
        let page = history
            .page_for_fleet(&workspace, &fleet, &filter, cursor.as_ref(), 1)
            .await
            .expect("a page reads");
        walked.extend(page.iter().map(|row| row.event_id.clone()));
        match next_cursor(&page, 1) {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(walked, newest_first, "each listed once, in keyset order");

    let first = ids.first().expect("two admissions were made");
    let found = history
        .detail(&workspace, &fleet, first)
        .await
        .expect("the detail reads")
        .expect("the row id names the event");
    assert_eq!(&found.row.event_id, first);
    let legacy = history
        .detail(&workspace, &fleet, LEGACY_ID)
        .await
        .expect("the detail reads");
    assert!(legacy.is_none(), "a {{millis}}-{{seq}} id names nothing");

    streams.forget(&lane.fleet).await.expect("cleanup");
    lane.cleanup().await;
}
