//! The statement counter, against the rig's Postgres.
//!
//! Marked `#[ignore]` so `make test-unit-all` compiles and lints this without
//! datastores; it needs the compose Postgres with `pg_stat_statements`
//! preloaded, which is what the rig starts.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod support;

use afd_bench::statements;
use sqlx::Acquire as _;

use self::support::{LANE, datastores};

/// The statement each step of the transaction runs.
const PROBE: &str = "SELECT 1";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_statement_counter_counts_each_statement() {
    // Held, because the tallies are database-wide: a lane measuring beside
    // this test would put its statements in this delta.
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    statements::install(&stores.database)
        .await
        .expect("the rig preloads pg_stat_statements");

    let before = statements::read(&stores.database)
        .await
        .expect("the counter answers");
    three_statements_in_one_transaction(&stores.database).await;
    let cost = statements::read(&stores.database)
        .await
        .expect("the counter answers")
        .since(before);

    assert_eq!(
        cost.statements, 3,
        "three statements, with BEGIN, COMMIT and the counter's own reads left out"
    );
    assert_eq!(cost.commits, 1, "one explicit transaction is one commit");
}

/// `BEGIN`, three statements, `COMMIT` — on one pooled connection, returned
/// before the counter reads.
async fn three_statements_in_one_transaction(database: &afd_db::Db) {
    let mut connection = database.acquire().await.expect("a pooled connection");
    let mut transaction = connection.begin().await.expect("a transaction opens");
    for _statement in 0..3 {
        sqlx::query(PROBE)
            .execute(&mut *transaction)
            .await
            .expect("the probe runs");
    }
    transaction.commit().await.expect("the transaction commits");
}
