//! The candidate scan's cursor-first sort key under the generic plan a
//! prepared statement settles on: the cursor's ids are cast once per scan,
//! not once for every row the sort reads.
//!
//! sqlx prepares `SELECT_READY_CANDIDATES` once per connection, and Postgres
//! may then plan it once for every bind. A sort key that casts the bound
//! `text[]` itself runs that cast, up to 64 ids, again for each candidate
//! row, up to 128 a poll. The text is prepared with the types sqlx binds and
//! explained with generic plans forced, in a transaction that rolls back.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet::lease::sql::lease::SELECT_READY_CANDIDATES;
use sqlx::{AssertSqlSafe, PgConnection, Row as _};

use crate::support::Fixtures;

/// Plans every prepared statement once for all binds, as a cached plan does.
const FORCE_GENERIC: &str = "SET LOCAL plan_cache_mode = force_generic_plan";
/// The scan's text prepared with the types sqlx binds it with.
const PREPARE: &str = "PREPARE ready_candidates \
(text, text, text[], bigint, bigint, bigint, text, text[]) AS ";
/// Its plan. The values are placeholders a generic plan never reads.
const EXPLAIN: &str = "EXPLAIN (VERBOSE, COSTS OFF) EXECUTE ready_candidates \
('active', '01890a5d-ac96-774b-bcce-b302099a80a1', '{}', 64, 0, 0, 'active', '{}')";
/// Drops the prepared text, which outlives the transaction.
const DEALLOCATE: &str = "DEALLOCATE ready_candidates";
/// The plan line naming what the scan sorts by.
const SORT_KEY: &str = "Sort Key:";
/// The bind carrying the cursor's partition's ids.
const CURSOR_IDS: &str = "$8";

/// Runs one statement that returns nothing worth reading. Every text is this
/// file's own constant or the scan's, never input.
async fn run(connection: &mut PgConnection, statement: &str) {
    sqlx::raw_sql(AssertSqlSafe(statement.to_owned()))
        .execute(&mut *connection)
        .await
        .unwrap_or_else(|failure| panic!("{statement}: {failure}"));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_cursor_ids_are_cast_once_per_scan() {
    let fixtures = Fixtures::create().await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    run(&mut connection, "BEGIN").await;
    run(&mut connection, FORCE_GENERIC).await;
    run(
        &mut connection,
        &format!("{PREPARE}{SELECT_READY_CANDIDATES}"),
    )
    .await;
    let plan: Vec<String> = sqlx::raw_sql(AssertSqlSafe(EXPLAIN))
        .fetch_all(&mut *connection)
        .await
        .expect("the scan explains")
        .iter()
        .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
        .collect();
    run(&mut connection, DEALLOCATE).await;
    run(&mut connection, "ROLLBACK").await;

    let sort_key = plan
        .iter()
        .find(|line| line.contains(SORT_KEY))
        .expect("the scan sorts");
    assert!(
        !sort_key.contains(CURSOR_IDS),
        "the cursor's ids are cast for every sorted row: {sort_key}"
    );
}
