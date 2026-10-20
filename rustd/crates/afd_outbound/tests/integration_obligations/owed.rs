//! Owing an answer and reading the ledger back: the steps every obligation
//! case is built from.
//!
//! Split from the obligation cases at the file cap.

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

/// Owes one delivery the way the report does — in a transaction that commits.
///
/// Returns whether this call is the one that created the row, which is what
/// `owe` answers and what the caller uses to decide whether to append. The row
/// is then [`AGED`](super::seed::AGED), as if the commit happened an hour ago.
pub(super) async fn owe_committed(harness: &OutboundHarness, row: &str, event_id: &str) -> bool {
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
    let written = obligation::owe(&mut transaction, row, delivery(event_id))
        .await
        .expect("owing a delivery");
    transaction
        .commit()
        .await
        .expect("the report's transaction commits");
    // Past both of the producer's spans, so a scan at production spans — and
    // the producer's own pass — offers it without a test waiting on a clock.
    age(&harness.owner, event_id).await;
    written
}

/// A fixture in the state each test starts from: parents seeded, nothing owed.
pub(super) async fn ready() -> OutboundHarness {
    let harness = OutboundHarness::reset().await;
    seed_parents(&harness.owner).await;
    clear_obligations(&harness.owner).await;
    harness
}

/// One of the producer's two recovery scans, as a value.
///
/// Named because the signature is unreadable inline and clippy says so: both
/// scans are `async fn`s with identical shapes, and the only way to pass either
/// to one body is as a function pointer returning a boxed future.
type Scan = for<'a> fn(
    &'a afd_db::Db,
    i64,
) -> std::pin::Pin<
    Box<dyn Future<Output = afd_outbound::Result<Vec<obligation::Owed>>> + Send + 'a>,
>;

/// The event ids one scan answers, oldest first.
///
/// Both scans take the same arguments and differ only in WHICH set they name,
/// so they share a body here for the same reason `obligation::write_receipt`
/// shares one between its two statements.
pub(super) async fn owing(harness: &OutboundHarness, scan: Scan) -> Vec<String> {
    scan(&harness.database, AMPLE)
        .await
        .expect("the scan answers")
        .into_iter()
        // The scans are deployment-wide by design; this suite asks about its own
        // fleet, the one `clear_obligations` resets (ISO-1).
        .filter(|owed| owed.fleet_id == FLEET)
        .map(|owed| owed.event_id)
        .collect()
}

/// Obligations still owed an APPEND — committed, never queued — at the age the
/// producer's own pass asks for.
pub(super) async fn awaiting_append(harness: &OutboundHarness) -> Vec<String> {
    owing(harness, |db, limit| {
        Box::pin(obligation::unreceipted(db, MIN_AGE, limit))
    })
    .await
}

/// Obligations still owed a DELIVERY — queued, nobody received them — at the
/// age the producer's own pass asks for.
pub(super) async fn awaiting_delivery(harness: &OutboundHarness) -> Vec<String> {
    owing(harness, |db, limit| {
        Box::pin(obligation::undelivered(db, LOST_AFTER, limit))
    })
    .await
}

/// Owes an answer AND puts it on the queue, the way the report's fast path does.
///
/// Three steps in the production order: the transaction commits the obligation,
/// the append follows it, and the receipt records which entry carries it. The
/// tests below each remove one of those and assert what is left.
pub(super) async fn owe_and_queue(harness: &OutboundHarness, nth: u8, event: &str) {
    assert!(
        owe_committed(harness, &obligation_id(nth), event).await,
        "each fixture answer owes its own delivery"
    );
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
    obligation::receipt(&harness.database, &obligation_id(nth), entry.as_str())
        .await
        .expect("recording the receipt");
    // The receipt moved `updated_at` to now; the entry has sat on the queue an
    // hour, as far as the undelivered scan can tell.
    age(&harness.owner, event).await;
}

/// A second handle on the lane's datastore, for the faults the queue cannot stage.
pub(super) async fn datastore() -> Dragonfly {
    Dragonfly::connect(&OutboundHarness::config())
        .await
        .expect("the lane's Dragonfly must be reachable")
}
