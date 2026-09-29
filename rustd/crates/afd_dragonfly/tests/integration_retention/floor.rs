//! Dimension 4.1 — the trim after an acknowledgement reads the entries it
//! removes plus one, stops that read at the oldest owed entry, and runs only
//! once a stream is past its bound plus [`TRIM_SLACK`].
//!
//! `Trimmed::read` witnesses the read's size; the stream's own entries
//! witness what survived, as in the suite above. The read cap,
//! `TRIM_READ_MAX`, is a pure bound and is proven beside it in the crate's
//! unit tests.

use afd_dragonfly::streams::{ACKNOWLEDGED_HISTORY, EventId, FleetStreams, TRIM_SLACK};

use super::append_many;
use crate::support::DragonflyHarness;

/// How far past the bound the first stream sits: 1,520 entries against 1,000.
const PAST_THE_BOUND: usize = 520;
/// Entries handed to a consumer and never acknowledged.
const PENDING: usize = 10;
/// Entries no consumer was handed.
const UNDELIVERED: usize = 10;
/// The one entry the second stream's consumer never acknowledges, far below
/// that stream's history window.
const STUCK: usize = 40;

/// A count as the width `Trimmed` and `Backlog` report in.
fn wide(count: usize) -> u64 {
    u64::try_from(count).expect("a test count fits")
}

/// Hands `receipts`' entries to `consumer`, asserting they arrive in append
/// order.
async fn deliver(streams: &FleetStreams, fleet: &str, consumer: &str, receipts: &[EventId]) {
    for expected in receipts {
        let event = streams
            .read_new(fleet, consumer)
            .await
            .expect("read")
            .expect("delivered in order");
        assert_eq!(&event.receipt, expected);
    }
}

/// Acknowledges every receipt given.
async fn ack_all<'a>(
    streams: &FleetStreams,
    fleet: &str,
    receipts: impl Iterator<Item = &'a EventId>,
) {
    for receipt in receipts {
        assert!(streams.ack(fleet, receipt).await.expect("ack"));
    }
}

/// Asserts the group still owes exactly `pending` and `undelivered`.
async fn assert_owes(streams: &FleetStreams, fleet: &str, pending: usize, undelivered: usize) {
    let backlog = streams
        .backlog(fleet)
        .await
        .expect("the group answers")
        .expect("the group exists");
    assert_eq!(
        backlog.pending,
        wide(pending),
        "the trim touched the pending list"
    );
    assert_eq!(
        backlog.undelivered,
        Some(wide(undelivered)),
        "the trim touched undelivered entries"
    );
}

/// 1,520 appended, 1,500 acknowledged, 10 pending, 10 undelivered: the trim
/// removes the 520 below the history window and reads 521. Then the slack:
/// at the bound plus [`TRIM_SLACK`] nothing is read, and one entry more trims
/// 101 while reading 102.
async fn the_window_is_read_and_nothing_more(harness: &DragonflyHarness) {
    let streams = FleetStreams::new(harness.redis.clone());
    let (fleet, consumer) = (harness.name("floor"), harness.name("consumer"));
    streams.ensure_group(&fleet).await.expect("group create");
    let appended = ACKNOWLEDGED_HISTORY + PAST_THE_BOUND;
    let receipts = append_many(&streams, &fleet, appended).await;
    let delivered = receipts.get(..appended - UNDELIVERED).expect("in range");
    deliver(&streams, &fleet, &consumer, delivered).await;
    ack_all(
        &streams,
        &fleet,
        delivered.iter().take(delivered.len() - PENDING),
    )
    .await;

    let trimmed = streams.trim(&fleet).await.expect("trim");
    assert_eq!(trimmed.removed, wide(PAST_THE_BOUND));
    assert_eq!(
        trimmed.read,
        wide(PAST_THE_BOUND + 1),
        "read the removed plus one"
    );
    assert_eq!(trimmed.retained, wide(ACKNOWLEDGED_HISTORY));
    assert_owes(&streams, &fleet, PENDING, UNDELIVERED).await;

    append_many(&streams, &fleet, TRIM_SLACK).await;
    let inside = streams.trim(&fleet).await.expect("trim");
    assert_eq!(
        (inside.removed, inside.read),
        (0, 0),
        "inside the slack: {inside:?}"
    );
    append_many(&streams, &fleet, 1).await;
    let past = streams.trim(&fleet).await.expect("trim");
    assert_eq!(past.removed, wide(TRIM_SLACK + 1));
    assert_eq!(
        past.read,
        wide(TRIM_SLACK + 2),
        "one past the slack: {past:?}"
    );
    assert_owes(&streams, &fleet, PENDING, UNDELIVERED + TRIM_SLACK + 1).await;
    streams.forget(&fleet).await.expect("cleanup");
}

/// One entry pending far below the history window: the read stops at it, so
/// the trim reads the entries below it plus that one, and removes only those
/// below it.
async fn the_read_stops_at_an_owed_entry(harness: &DragonflyHarness) {
    let streams = FleetStreams::new(harness.redis.clone());
    let (fleet, consumer) = (harness.name("stuck"), harness.name("consumer"));
    streams.ensure_group(&fleet).await.expect("group create");
    let appended = ACKNOWLEDGED_HISTORY + TRIM_SLACK + 1;
    let receipts = append_many(&streams, &fleet, appended).await;
    deliver(&streams, &fleet, &consumer, &receipts).await;
    let stuck = receipts.get(STUCK).expect("in range");
    ack_all(
        &streams,
        &fleet,
        receipts.iter().filter(|receipt| *receipt != stuck),
    )
    .await;

    let trimmed = streams.trim(&fleet).await.expect("trim");
    assert_eq!(
        trimmed.removed,
        wide(STUCK),
        "only entries below the owed one"
    );
    assert_eq!(
        trimmed.read,
        wide(STUCK + 1),
        "the read stopped at the owed entry"
    );
    assert!(
        streams
            .holds_entry(&fleet, stuck)
            .await
            .expect("range read")
    );
    let below = receipts.get(STUCK - 1).expect("in range");
    assert!(
        !streams
            .holds_entry(&fleet, below)
            .await
            .expect("range read")
    );
    assert_owes(&streams, &fleet, 1, 0).await;
    streams.forget(&fleet).await.expect("cleanup");
}

/// Dimension 4.1.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_trim_reads_only_the_floor() {
    let harness = DragonflyHarness::connect().await;
    the_window_is_read_and_nothing_more(&harness).await;
    the_read_stops_at_an_owed_entry(&harness).await;
}
