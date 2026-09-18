//! The delivery stamp's index, and the predicate it was NOT allowed to gain.
//!
//! # Why the plan is not asserted here
//!
//! The obvious test is `EXPLAIN` over the stamp, asserting it names the new
//! index. On this suite's fixtures it would assert nothing: a table holding a
//! handful of rows is read fastest by a sequential scan and PostgreSQL says so,
//! correctly. The two ways to make such a test go green are to seed a
//! production-sized table on every run, or to `SET enable_seqscan = off` — and
//! the second is the test writing the answer it wanted to read.
//!
//! So the plan evidence is a MEASUREMENT, recorded where a measurement belongs:
//! the Pull Request and the slot's own comment carry the before and after from
//! a 400,600-row table. What is asserted here is what stays true at any size —
//! that the index exists, that its columns and predicate are the ones the
//! stamp's own predicate implies, and that the stamp still marks the rows it
//! has to mark.
//!
//! # The predicate the stamp must never gain
//!
//! `idx_fleet_admissions_undelivered` could not serve the stamp because it is
//! partial on `receipt IS NOT NULL` and the stamp says nothing about the
//! receipt. The cheap fix is to add that clause to the stamp, and it is wrong:
//! a delivery can be recorded before the append writes its receipt back, and
//! replay can replace a receipt under a row that was already delivered. Slot
//! 914 moves the index to the query instead. The second test here is what stops
//! the cheap fix coming back.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::{self, UnixMillis};
use afd_dragonfly::FleetStreams;
use sqlx::Row as _;

use crate::integration_admission_recovery::{
    EVERY_FLEET, EVERY_ROW, NO_GRACE, RECOVERY_LANE, admission, deferring, ledger, producer_key,
};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// The index slot 914 adds.
const LOOKUP_INDEX: &str = "idx_fleet_admissions_delivery_lookup";

/// The columns the stamp keys on, in the order it keys on them.
const LOOKUP_COLUMNS: &str = "(fleet_id, created_at, seq)";

/// The predicate the stamp's own `WHERE` implies, and the whole of it.
const LOOKUP_PREDICATE: &str = "WHERE (delivered_at IS NULL)";

/// The clause that must NOT appear: it is what made the old index unusable.
const RECEIPT_CLAUSE: &str = "receipt";

/// The index's definition as PostgreSQL stores it.
async fn definition(fixtures: &Fixtures, index: &str) -> String {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    let row = sqlx::query("SELECT indexdef FROM pg_indexes WHERE indexname = $1")
        .bind(index)
        .fetch_one(&mut *connection)
        .await
        .unwrap_or_else(|failure| panic!("{index} must exist after migration: {failure}"));
    row.try_get::<String, _>(0)
        .expect("an index definition is text")
}

/// Slot 914's index is the one the delivery stamp's predicate implies.
///
/// Asserted against `pg_indexes` rather than against the file, so what is
/// graded is the index the database actually built. The negative half carries
/// the point: a definition mentioning the receipt would be back to an index the
/// stamp cannot use, whatever its name promised.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_delivery_lookup_index_implies_the_stamps_predicate() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;

    let lookup = definition(&fixtures, LOOKUP_INDEX).await;
    assert!(
        lookup.contains(LOOKUP_COLUMNS),
        "the stamp keys on the fleet and the logical id's two integers: {lookup}"
    );
    assert!(
        lookup.contains(LOOKUP_PREDICATE),
        "the predicate must be the one the stamp implies, and only that: {lookup}"
    );
    assert!(
        !lookup.contains(RECEIPT_CLAUSE),
        "a receipt clause is what made the older index unusable for the stamp: {lookup}"
    );

    // The older index keeps its stricter predicate; the recovery reads imply it
    // and nothing in this slot was supposed to touch them.
    let undelivered = definition(&fixtures, "idx_fleet_admissions_undelivered").await;
    assert!(
        undelivered.contains(RECEIPT_CLAUSE),
        "the reconciliation index still carries the receipt test it reads on: {undelivered}"
    );

    fixtures.cleanup().await;
}

/// The stamp marks a delivery whose receipt is absent, and one replay replaced.
///
/// Both are rows the tempting `receipt IS NOT NULL` fix would have stopped
/// marking. The first is the window between the row committing and the append
/// writing its entry id back. The second is a row recovery re-appended, whose
/// stored receipt is not the one it was admitted with — keyed on the logical
/// event id, the stamp does not care, and that is the property being pinned.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn delivery_stamps_before_and_after_a_replayed_receipt() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    // ── A row that has no receipt yet: the queue was not there to give one.
    let unreceipted_key = producer_key(&fleet, "stamp-unreceipted");
    let unreceipted = deferring(&fixtures)
        .admit(admission(&fleet, &workspace, &unreceipted_key))
        .await
        .expect("the row commits whatever the queue does");
    assert_eq!(
        fixtures.admission_receipt(&fleet, &unreceipted.id).await,
        None,
        "an append that never happened records no receipt"
    );

    // ── A row whose receipt recovery replaced with a different one.
    let replayed_key = producer_key(&fleet, "stamp-replayed");
    let replayed = live
        .admit(admission(&fleet, &workspace, &replayed_key))
        .await
        .expect("a live queue admits and receipts");
    let first_receipt = fixtures
        .admission_receipt(&fleet, &replayed.id)
        .await
        .expect("a live append records its receipt");
    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");
    let at = clock::now();
    live.reconcile(
        at,
        EVERY_FLEET,
        EVERY_ROW,
        &mut afd_admission::Progress::default(),
    )
    .await
    .expect("the reconcile pass runs");
    live.replay(at, NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs");
    let second_receipt = fixtures
        .admission_receipt(&fleet, &replayed.id)
        .await
        .expect("replay re-appended the row and recorded a new receipt");
    assert_ne!(
        first_receipt, second_receipt,
        "the point of this row is that its receipt MOVED"
    );

    // ── The stamp, run as the lease path runs it, against both rows.
    for event_id in [&unreceipted.id, &replayed.id] {
        assert_eq!(
            stamp(&fixtures, &fleet, event_id).await,
            1,
            "{event_id} is undelivered and must be stamped, receipt or no receipt"
        );
        assert!(
            fixtures
                .admission_delivered_at(&fleet, event_id)
                .await
                .is_some(),
            "{event_id} carries the instant it was delivered"
        );
        assert_eq!(
            stamp(&fixtures, &fleet, event_id).await,
            0,
            "{event_id} is already stamped, so a second delivery moves nothing"
        );
    }

    fixtures.cleanup().await;
}

/// Runs `MARK_DELIVERED` for one logical event, answering rows affected.
///
/// Binds the shipped statement rather than a copy of it, so a predicate added
/// to the real one fails here instead of passing against a stale duplicate.
async fn stamp(fixtures: &Fixtures, fleet: &str, event_id: &str) -> u64 {
    let (created_at, seq) = event_id
        .split_once('-')
        .expect("a logical event id is `<created_at>-<seq>`");
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    sqlx::query(afd_admission::sql::MARK_DELIVERED)
        .bind(fleet)
        .bind(created_at.parse::<i64>().expect("the instant is numeric"))
        .bind(seq.parse::<i64>().expect("the sequence is numeric"))
        .bind(UnixMillis::from_millis(clock::now().as_millis()).as_millis())
        .execute(&mut *connection)
        .await
        .expect("the stamp runs")
        .rows_affected()
}
