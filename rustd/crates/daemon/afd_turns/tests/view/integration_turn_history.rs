//! An admitted event is named by its `thread.turns` row id,
//! and the history keyset `(created_at, turn_id)` reads two such events in one
//! millisecond newest-first, each once, across a page boundary. An id no
//! turn minted names nothing.
//!
//! Lives here rather than beside the receipts in `afd_turns`, because the
//! history read is this crate's and `afd_turns` cannot depend on it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_wire::turn::TurnKind;
use afd_db::test_util::mint_id;
use afd_turns::view::{Filter, History, THREAD_MAX_LIMIT, next_cursor};
use afd_turns::{IncomingTurn, Key, Producer};
use jiff::Timestamp;

use crate::support::TurnsLane;

/// The one millisecond both events are recorded at.
const SHARED_INSTANT: Timestamp = Timestamp::constant(1_800_000_000, 7_000_000);

fn turn<'a>(lane: &'a TurnsLane, key: &'a str) -> IncomingTurn<'a> {
    IncomingTurn {
        producer: Producer::Steer,
        key: Key::Repeated(key),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: "steer:api",
        event_type: TurnKind::Chat,
        request_json: r#"{"message":"history fixture"}"#,
        reply: afd_turns::Reply::None,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_turns_in_one_millisecond_list_once_newest_first() {
    let lane = TurnsLane::open().await;
    let ledger = lane.turns();
    let mut ids = Vec::new();
    for name in ["first", "second"] {
        let key = format!("{}-{name}", lane.fleet);
        let admitted = ledger
            .admit(turn(&lane, &key))
            .await
            .expect("the turn is taken");
        // Delivered as the worker records it: under the turn's own id,
        // both at one instant so only the id can break the tie.
        lane.seed_event(&admitted.stored.id, SHARED_INSTANT).await;
        ids.push(admitted.stored.id);
    }
    let mut newest_first = ids.clone();
    newest_first.sort_by(|older, newer| newer.cmp(older));

    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (&lane.workspace, &lane.fleet);
    let filter = Filter::default();

    let whole = history
        .page_for_workspace(workspace, Some(fleet), &filter, None, 10)
        .await
        .expect("a page reads");
    let listed: Vec<_> = whole.iter().map(|row| row.turn_id.clone()).collect();
    assert_eq!(listed, newest_first);

    // One row a page: the cursor pair carries the tie across the boundary.
    let mut walked = Vec::new();
    let mut cursor = None;
    loop {
        let page = history
            .page_for_workspace(workspace, Some(fleet), &filter, cursor.as_ref(), 1)
            .await
            .expect("a page reads");
        walked.extend(page.iter().map(|row| row.turn_id.clone()));
        match next_cursor(&page, 1) {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(walked, newest_first, "each listed once, in keyset order");

    let first = ids.first().expect("two turns were made");
    let found = history
        .detail(workspace, fleet, first)
        .await
        .expect("the detail reads")
        .expect("the row id names the event");
    assert_eq!(&found.row.turn_id, first);
    let unadmitted = history
        .detail(workspace, fleet, &mint_id())
        .await
        .expect("the detail reads");
    assert!(unadmitted.is_none(), "an id no turn minted names nothing");

    lane.cleanup().await;
}

/// A thread page serves the largest size the route accepts: the store reads
/// that many rows and the one past them that says a next page exists.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_thread_page_of_100_is_served() {
    let lane = TurnsLane::open().await;
    let ledger = lane.turns();
    let stored = THREAD_MAX_LIMIT + 2;
    for number in 0..stored {
        let key = format!("{}-page-{number}", lane.fleet);
        let admitted = ledger
            .admit(turn(&lane, &key))
            .await
            .expect("the turn is taken");
        lane.seed_event(&admitted.stored.id, SHARED_INSTANT).await;
    }

    let history = History::new(lane.database.clone());
    let page = history
        .thread_page(&lane.workspace, &lane.fleet, None, THREAD_MAX_LIMIT + 1)
        .await
        .expect("a page reads");

    let read = i64::try_from(page.len()).expect("a page length fits an i64");
    assert_eq!(read, THREAD_MAX_LIMIT + 1, "a full page and the row past it");

    lane.cleanup().await;
}
