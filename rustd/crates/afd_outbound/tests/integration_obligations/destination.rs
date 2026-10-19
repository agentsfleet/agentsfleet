//! Where an owed answer goes: the producer appends it with its destination,
//! a requeue keeps it, and a row naming no connector is given up.
//!
//! Split from the obligation cases at the file cap.

use super::*;

/// Dimension 7.6 — the DAEMON's producer appends it, and this test does not.
///
/// Every other case in this file calls a scan and then enqueues by hand. That
/// grades `obligation::unreceipted` and the queue, and it is exactly why "a
/// daemon producer enqueues it" had no proof. `Producer::run` is the loop
/// `agentsfleetd` spawns (`agentsfleetd/src/outbound.rs:95`); it scans at its
/// OWN spans, measured on the database clock, rather than taking them as parameters, it
/// appends every row a scan answers, and it records each entry as that row's
/// receipt. None of that is reached by calling a scan directly, and none of it
/// had a test: nothing under `tests/` named `afd_outbound::producer`, and
/// `producer.rs` carries no `#[cfg(test)]` module.
///
/// Dimensions 4.2 and 4.3 are closed by THIS Dimension on the finding that
/// nothing in the daemon called `OutboundQueue::enqueue`. This is the test that
/// fails if that becomes true again.
///
/// No clock is waited on. The owed row is aged past `producer::MIN_AGE`, so the
/// producer's first pass already finds it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn the_daemon_producer_appends_an_owed_answer_and_receipts_it() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-6";

    assert!(owe_committed(&harness, &obligation_id(8), event).await);
    assert_eq!(entries_on(&redis).await, 0, "nothing has appended it yet");

    let appended = producer_until(&harness, async || entries_on(&redis).await > 0).await;
    assert!(
        appended,
        "the producer's own pass appended nothing: the daemon path that closes \
         Dimensions 4.2 and 4.3 never reached `OutboundQueue::enqueue`"
    );
    assert_eq!(
        entries_naming(&redis, event).await,
        1,
        "the owed answer was appended exactly once — asked about THIS event \
         rather than about the stream, which the producer also fills with every \
         other fleet's owed answers, correctly"
    );
    assert!(
        awaiting_append(&harness).await.is_empty(),
        "the producer receipted the row it appended, so the next pass does not \
         append the same answer a second time"
    );
}

/// Dimension 3.3 — the producer re-appends an owed answer carrying the
/// destination its row stored, so a poster reading only the job still threads
/// it where the question came from.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn requeued_obligation_keeps_its_destination() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-7";

    assert!(owe_committed(&harness, &obligation_id(9), event).await);
    let appended = producer_until(&harness, async || entries_naming(&redis, event).await > 0).await;
    assert!(appended, "the producer's pass appended the owed answer");
    assert_eq!(
        destinations_naming(&redis, event).await,
        vec![DESTINATION.to_owned()],
        "the re-appended job carries the destination the row stored"
    );
}

/// A row whose stored connector no connector answers to is abandoned by the
/// producer, not appended and not left to come back in every scan, where a
/// batch of them would fill the pass and starve every answer behind it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn an_unaddressable_row_is_abandoned_not_requeued() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-8";
    // What a connector removed from the catalogue leaves behind: a row naming
    // a destination and a connector nobody answers to.
    seed_owed_row(
        &harness.database,
        OwedRow {
            nth: 10,
            event,
            provider: UNKNOWN_CONNECTOR,
            destination: DESTINATION,
            receipt: None,
            answer: ANSWER,
        },
    )
    .await;

    let abandoned = producer_until(&harness, async || {
        abandonment(&harness.database, event).await.1.is_some()
    })
    .await;
    assert!(abandoned, "the producer's pass abandoned the row");
    assert_eq!(
        abandonment(&harness.database, event).await.1.as_deref(),
        Some(AbandonReason::Unaddressable.as_str())
    );
    assert_eq!(
        entries_naming(&redis, event).await,
        0,
        "nothing was appended for an answer no poster could take"
    );
}

/// The trigger standing in for a ledger that takes the append's receipt badly.
const RECEIPT_REFUSAL: &str = "afd_it_outbound_receipt_refused";

/// A re-append whose receipt the ledger refuses is still on the queue, and the
/// row stays owed: the next pass appends it again, and the destination may see
/// the answer twice — the direction the producer already accepts, over losing
/// it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_reappend_whose_receipt_is_refused_stays_owed() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-9";
    assert!(owe_committed(&harness, &obligation_id(11), event).await);
    refuse_receipts(&harness, Some(event)).await;

    let appended = producer_until(&harness, async || entries_naming(&redis, event).await > 0).await;
    refuse_receipts(&harness, None).await;

    assert!(appended, "the append itself went through");
    assert_eq!(
        awaiting_append(&harness).await,
        vec![event.to_owned()],
        "the refused receipt leaves the row for the next pass"
    );
}

/// Installs, for `event`, or removes, for `None`, a trigger refusing every
/// receipt written to that event's row.
async fn refuse_receipts(harness: &OutboundHarness, event: Option<&str>) {
    let mut connection = harness
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    let mut statements = vec![
        format!("DROP TRIGGER IF EXISTS {RECEIPT_REFUSAL} ON fleet.obligations"),
        format!("DROP FUNCTION IF EXISTS public.{RECEIPT_REFUSAL}()"),
    ];
    // The event is this suite's own constant, never input.
    if let Some(event) = event {
        statements.push(format!(
            "CREATE FUNCTION public.{RECEIPT_REFUSAL}() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN RAISE EXCEPTION 'receipt fixture refusal'; END $$"
        ));
        statements.push(format!(
            "CREATE TRIGGER {RECEIPT_REFUSAL} BEFORE UPDATE ON fleet.obligations FOR EACH ROW \
             WHEN (NEW.event_id = '{event}' AND NEW.receipt IS NOT NULL) \
             EXECUTE FUNCTION public.{RECEIPT_REFUSAL}()"
        ));
    }
    for statement in statements {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&mut *connection)
            .await
            .expect("the receipt fault installs and removes");
    }
}

/// Runs the daemon's own producer until `done` holds or [`PRODUCER_PASS`]
/// runs out, and answers whether it held.
///
/// The producer is cancelled and joined before this returns, so a case's
/// failing assertion still stops the loop rather than leaving it appending
/// under the next test's lane lock.
async fn producer_until(harness: &OutboundHarness, mut done: impl AsyncFnMut() -> bool) -> bool {
    let token = CancellationToken::new();
    let running = tokio::spawn(
        Producer::new(harness.queue.clone(), harness.database.clone()).run(token.clone()),
    );
    let held = timeout(PRODUCER_PASS, async {
        while !done().await {
            sleep(POLL).await;
        }
    })
    .await;
    token.cancel();
    running
        .await
        .expect("the producer stops when its token is cancelled");
    held.is_ok()
}

/// A connector id nothing in the catalogue answers to.
const UNKNOWN_CONNECTOR: &str = "carrier-pigeon";
