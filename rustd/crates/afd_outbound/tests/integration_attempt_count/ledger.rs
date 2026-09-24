//! The ledger half's steps: owe an answer, queue it, read its row back.
//!
//! Split from the attempt-count cases at the file cap; the ledger, worker and
//! abandonment cases all build on these.

use super::capture::*;
use super::*;

/// One owed answer, addressed.
pub(super) fn delivery(event_id: &str) -> Delivery<'_> {
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
pub(super) async fn ready() -> OutboundHarness {
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
pub(super) async fn owe_and_queue(harness: &OutboundHarness, nth: u8, event: &str) -> EventId {
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
pub(super) async fn row(harness: &OutboundHarness, event: &str) -> (i64, Option<i64>, i64) {
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
pub(super) async fn reoffered_before(harness: &OutboundHarness, cutoff: i64) -> Vec<String> {
    obligation::undelivered(&harness.database, UnixMillis::from_millis(cutoff), AMPLE)
        .await
        .expect("the scan answers")
        .into_iter()
        // Deployment-wide scan, this suite's own fleet (ISO-1).
        .filter(|owed| owed.fleet_id == FLEET)
        .map(|owed| owed.event_id)
        .collect()
}
