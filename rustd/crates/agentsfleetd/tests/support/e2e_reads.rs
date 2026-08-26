//! The datastore half of §7: what the loop left behind.
//!
//! Split from the suites by concern rather than by size (RULE FLL): every
//! function here reads a ROW through the pool the BOOTED daemon opened, so an
//! assertion sees what the request actually wrote rather than what a second
//! connection can see. The request half is `e2e_wire.rs`.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use sqlx::{AssertSqlSafe, Row as _};

use crate::e2e::Scenario;

/// Asserts that exactly the recorded columns are populated on the matched row.
///
/// Derived from `information_schema` rather than from a hand-listed set, so a
/// column a MIGRATION adds is caught too: a new column nothing writes shows up
/// as a shape the recorded list does not carry, and the person who added it
/// decides whether the statement should fill it. A hand-maintained list on both
/// sides would agree with itself forever.
pub(crate) async fn assert_shape(
    run: &Scenario,
    table: &str,
    predicate: &str,
    bind: &str,
    recorded: &[&str],
) {
    let (schema, name) = table.split_once('.').expect("a qualified table name");
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");

    let columns: Vec<String> = sqlx::query(
        "SELECT column_name FROM information_schema.columns
          WHERE table_schema = $1 AND table_name = $2
          ORDER BY ordinal_position",
    )
    .bind(schema)
    .bind(name)
    .fetch_all(&mut *connection)
    .await
    .expect("the catalogue read must run")
    .into_iter()
    .map(|row| row.try_get::<String, _>(0).expect("a column name is text"))
    .collect();
    assert!(
        !columns.is_empty(),
        "{table} has no columns — the migrations did not apply"
    );

    // One statement, one row: every column asked for at once, so the answer
    // cannot straddle two snapshots of a row a sweeper might be touching.
    //
    // `ORDER BY created_at, id` is NOT cosmetic. `fetch_optional` takes the
    // first row the server happens to return and ignores the rest, and two of
    // these predicates legitimately match more than one row — a runner writes a
    // narrative row at enrolment and another at claim. Without an order the
    // suite asserts against whichever row Postgres felt like returning, which
    // is a test that passes until it does not. `id` breaks the tie because
    // `created_at` is a millisecond and two rows can share one.
    let projection = columns
        .iter()
        .map(|column| format!("({column} IS NOT NULL)::text"))
        .collect::<Vec<_>>()
        .join(", ");
    let statement = AssertSqlSafe(format!(
        "SELECT {projection} FROM {table} WHERE {predicate} ORDER BY created_at, id LIMIT 1"
    ));
    let row = sqlx::query(statement)
        .bind(bind)
        .fetch_optional(&mut *connection)
        .await
        .expect("the shape read must run")
        .unwrap_or_else(|| panic!("{table} carries no row matching {predicate}"));

    let populated: Vec<&str> = columns
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            row.try_get::<String, _>(*index)
                .expect("a boolean rendered as text")
                == "true"
        })
        .map(|(_, column)| column.as_str())
        .collect();

    assert_eq!(
        populated, recorded,
        "{table}'s populated columns drifted from the recorded shape — a ported \
         statement stopped filling one, or a migration added one nothing writes"
    );
}

/// One column of a lease row, as text.
pub(crate) async fn lease_column(run: &Scenario, lease: &str, column: &str) -> Option<String> {
    let statement = AssertSqlSafe(format!(
        "SELECT {column}::text FROM fleet.runner_leases WHERE id = $1::uuid"
    ));
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(statement)
        .bind(lease)
        .fetch_optional(&mut *connection)
        .await
        .expect("the lease read must run")
        .map(|row| row.try_get(0).expect("the column must be readable as text"))
}

/// The scenario tenant's current balance.
pub(crate) async fn balance(run: &Scenario) -> Option<i64> {
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("SELECT balance_nanos FROM billing.tenant_wallet WHERE tenant_id = $1::uuid")
        .bind(&run.tenant)
        .fetch_optional(&mut *connection)
        .await
        .expect("the balance read must run")
        .map(|row| row.try_get(0).expect("the balance is a bigint"))
}

/// How many ledger rows the scenario's event has, across every charge type.
pub(crate) async fn ledger_rows(run: &Scenario) -> i64 {
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("SELECT count(*) FROM billing.usage_ledger WHERE event_id = $1")
        .bind(&run.event_id)
        .fetch_one(&mut *connection)
        .await
        .expect("the ledger count must run")
        .try_get(0)
        .expect("a count is a bigint")
}

/// One column of the scenario runner's lifetime counters, as text.
pub(crate) async fn counter_column(run: &Scenario, column: &str) -> Option<String> {
    let statement = AssertSqlSafe(format!(
        "SELECT {column}::text FROM fleet.runner_lifetime_counters WHERE runner_id = $1::uuid"
    ));
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(statement)
        .bind(run.runner_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .expect("the counter read must run")
        .map(|row| row.try_get(0).expect("the column must be readable as text"))
}
