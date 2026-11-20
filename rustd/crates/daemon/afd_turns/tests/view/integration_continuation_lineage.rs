//! What a continuation's row remembers when two writers race for it.
//!
//! # The race, and why ordering cannot settle it
//!
//! A resolved gate lands a continuation through two independent writers. The
//! approval path knows the predecessor — the event a person unblocked — and
//! binds it. The run path knows nothing about it: a runner polling the fleet
//! can pick the continuation up the instant `admit()` marks the fleet ready,
//! which is BEFORE the approval path writes its row, and its insert binds no
//! predecessor at all.
//!
//! Both run `INSERT_TURN` against the same turn `id`. Only
//! one of them can be first, and no amount of reordering fixes that: the two
//! live in different processes with a network between them. So the row itself
//! has to hold the answer, which is what the conflict arm's `COALESCE` does.
//!
//! # What these prove
//!
//! That the surviving row carries the predecessor under BOTH orders, that a
//! writer arriving without one cannot erase one already stored, and that
//! `inserted` still tells the write that opened the event from a redelivery now
//! that both arms return a row. The last one is load-bearing: two callers read that flag where
//! they read `rows_affected()` before, and `rows_affected()` reports one on
//! either arm once the conflict arm updates.
//!
//! Reverting the conflict arm to `DO NOTHING` fails the first, third and fourth
//! tests here, which is what makes them tests.
//!
//! Marked `#[ignore]` so `make test-unit-all` still COMPILES and lints this
//! without datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use sqlx::Row as _;

use crate::support::TurnsLane;

/// The event a person unblocked, which the continuation resumes: a turn of
/// its own, since `resumes_turn_id` names a row of `thread.turns`.
async fn seed_predecessor(lane: &TurnsLane) -> Uuid7 {
    let predecessor = afd_db::test_util::mint_id();
    insert(lane, &predecessor, None).await;
    predecessor
}

/// Runs the shared narrative insert and answers whether it inserted a row.
///
/// The binds mirror the production caller exactly — `afd_approval`'s
/// `continue_from` — because a fixture that bound its own column order would
/// prove something about itself.
async fn insert(lane: &TurnsLane, turn_id: &Uuid7, resumes: Option<&Uuid7>) -> bool {
    let mut connection = lane.connection().await;
    sqlx::query(afd_turns::view::sql::INSERT_TURN)
        .bind(&lane.fleet)
        .bind(turn_id)
        .bind(&lane.workspace)
        .bind("continuation:test")
        .bind("continuation")
        .bind(sqlx::types::Json(serde_json::json!({})))
        .bind(resumes)
        .bind(af_core::turn::status::RECEIVED)
        .fetch_one(&mut *connection)
        .await
        .expect("the narrative insert must run")
        .try_get(0)
        .expect("the statement returns whether it inserted")
}

/// What the row under `turn_id` names as its predecessor.
async fn stored_predecessor(lane: &TurnsLane, turn_id: &Uuid7) -> Option<Uuid7> {
    let mut connection = lane.connection().await;
    sqlx::query(
        "SELECT resumes_turn_id FROM thread.turns
         WHERE thread_id = $1 AND id = $2",
    )
    .bind(&lane.fleet)
    .bind(turn_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the row must be readable")
    .try_get(0)
    .expect("resumes_turn_id is a nullable uuid column")
}

/// How many rows the pair of writes left behind.
///
/// Asserted everywhere below, because a conflict arm that stopped conflicting
/// would satisfy every lineage assertion here by writing a second row.
async fn row_count(lane: &TurnsLane, turn_id: &Uuid7) -> i64 {
    let mut connection = lane.connection().await;
    sqlx::query(
        "SELECT count(*) FROM thread.turns
         WHERE thread_id = $1 AND id = $2",
    )
    .bind(&lane.fleet)
    .bind(turn_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the count must run")
    .try_get(0)
    .expect("count answers a bigint")
}

/// The run path wins the race; the approval path still records the lineage.
///
/// This is the order that lost the link outright before the conflict arm
/// converged: the runner's insert landed first with no predecessor, and the
/// approval path's insert was discarded whole.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn lineage_survives_run_before_approval() {
    let lane = TurnsLane::open().await;
    let event = afd_db::test_util::mint_id();
    let predecessor = seed_predecessor(&lane).await;

    let first = insert(&lane, &event, None).await;
    let second = insert(&lane, &event, Some(&predecessor)).await;

    assert!(first, "the runner's insert opens the row");
    assert!(!second, "the approval path finds the row already there");
    assert_eq!(
        stored_predecessor(&lane, &event).await,
        Some(predecessor),
        "the predecessor the approval path knew must reach the row"
    );
    assert_eq!(row_count(&lane, &event).await, 1, "one event, one row");

    lane.cleanup().await;
}

/// The approval path wins the race; the run path leaves the lineage alone.
///
/// This order always kept the link. It is asserted so that the fix cannot be
/// read as having traded one order for the other.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn lineage_kept_when_approval_writes_first() {
    let lane = TurnsLane::open().await;
    let event = afd_db::test_util::mint_id();
    let predecessor = seed_predecessor(&lane).await;

    let first = insert(&lane, &event, Some(&predecessor)).await;
    let second = insert(&lane, &event, None).await;

    assert!(first, "the approval path opens the row");
    assert!(!second, "the runner finds the row already there");
    assert_eq!(
        stored_predecessor(&lane, &event).await,
        Some(predecessor),
        "a writer with no predecessor must not clear one"
    );
    assert_eq!(row_count(&lane, &event).await, 1, "one event, one row");

    lane.cleanup().await;
}

/// A redelivery that knows no predecessor never clears one already stored.
///
/// The run path re-runs this statement on every reclaim and every re-poll,
/// always binding `None`. A conflict arm that overwrote rather than coalesced
/// would erase the lineage on the first redelivery after the answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_redelivery_never_clears_lineage() {
    let lane = TurnsLane::open().await;
    let event = afd_db::test_util::mint_id();
    let predecessor = seed_predecessor(&lane).await;

    insert(&lane, &event, Some(&predecessor)).await;
    for _ in 0..3 {
        assert!(
            !insert(&lane, &event, None).await,
            "every redelivery takes the conflict arm"
        );
    }

    assert_eq!(
        stored_predecessor(&lane, &event).await,
        Some(predecessor),
        "three redeliveries must leave the predecessor where it was"
    );
    assert_eq!(row_count(&lane, &event).await, 1, "one event, one row");

    lane.cleanup().await;
}

/// The returned flag says what `rows_affected` used to say, and nothing else.
///
/// Both callers gate a tail frame, a counter read and a receive debit on this
/// answer, so a flag that read true on the conflict arm would announce a
/// redelivered event and move the counters a second time.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn only_a_fresh_insert_reports_inserted() {
    let lane = TurnsLane::open().await;
    let event = afd_db::test_util::mint_id();
    let predecessor = seed_predecessor(&lane).await;

    assert!(
        insert(&lane, &event, None).await,
        "the first write inserts the row"
    );
    assert!(
        !insert(&lane, &event, None).await,
        "an identical second write inserts nothing"
    );
    assert!(
        !insert(&lane, &event, Some(&predecessor)).await,
        "a converging write is still not an insert"
    );

    lane.cleanup().await;
}

/// The event id is the row's key: the insert stores the id it was given on the
/// fresh arm and keeps the SAME row on the conflict arm, so a caller pointing a
/// tool call or a run at the row gets one identity however many times the
/// event arrives.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_turn_id_is_the_row_key_and_stable_across_arms() {
    let lane = TurnsLane::open().await;
    let event = afd_db::test_util::mint_id();

    let first = inserted_id(&lane, &event).await;
    let again = inserted_id(&lane, &event).await;

    assert_eq!(first, event, "a fresh insert answers the event's own id");
    assert_eq!(again, first, "a redelivery answers the row's own id");
    assert_eq!(row_count(&lane, &event).await, 1);

    lane.cleanup().await;
}

/// Runs the shared insert, then reads back the id of the row it wrote.
async fn inserted_id(lane: &TurnsLane, turn_id: &Uuid7) -> Uuid7 {
    let mut connection = lane.connection().await;
    sqlx::query(afd_turns::view::sql::INSERT_TURN)
        .bind(&lane.fleet)
        .bind(turn_id)
        .bind(&lane.workspace)
        .bind("continuation:test")
        .bind("continuation")
        .bind(sqlx::types::Json(serde_json::json!({})))
        .bind(None::<&Uuid7>)
        .bind(af_core::turn::status::RECEIVED)
        .execute(&mut *connection)
        .await
        .expect("the narrative insert must run");
    sqlx::query(
        "SELECT id AS turn_id FROM thread.turns
         WHERE thread_id = $1 AND id = $2",
    )
    .bind(&lane.fleet)
    .bind(turn_id)
    .fetch_one(&mut *connection)
    .await
    .expect("the row the insert wrote must read back")
    .try_get("turn_id")
    .expect("the row carries its event id")
}
