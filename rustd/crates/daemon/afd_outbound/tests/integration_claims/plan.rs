//! The lane-head claim reads the open-lane index over a realistic spread:
//! many lanes, most of their rows delivered, one open head each.

use afd_db::test_util::mint_id;
use afd_outbound::obligation;

use crate::{Ledger, sql};

/// Lanes the plan's rows are dealt round.
const LANES: usize = 100;

/// 10,100 rows dealt round the private database's fleets, one lane each: the
/// first 10,000 delivered, the last hundred one open head per lane.
const SEED_OBLIGATIONS: &str = "INSERT INTO fleet.obligations (id, fleet_id, workspace_id, \
    provider, destination, event_id, answer, receipt, claim_expires_at, delivered_at, \
    attempt_count, created_at, updated_at) \
    SELECT ('0195b4ba-8d3a-7a11-8abc-' || lpad(to_hex(n), 12, '0'))::uuid, f.id, \
    f.workspace_id, 'slack', 'd', \
    ('0195b4ba-8d3a-7a12-8abc-' || lpad(to_hex(n), 12, '0'))::uuid, 'a', \
    ('0195b4ba-8d3a-7a13-8abc-' || lpad(to_hex(n), 12, '0'))::uuid, now(), \
    CASE WHEN n <= 10000 THEN now() END, 0, now(), now() \
    FROM generate_series(1, 10100) n \
    JOIN (SELECT id, workspace_id, row_number() OVER (ORDER BY id) - 1 AS lane \
          FROM fleet.fleets) f ON f.lane = n % (SELECT count(*) FROM fleet.fleets)";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn claim_reads_the_open_lane_index() {
    let ledger = Ledger::create().await;
    for _ in 0..LANES {
        ledger.lane().await;
    }
    sql(&ledger.database, SEED_OBLIGATIONS, &[]).await;
    let mut connection = ledger.database.acquire().await.expect("a connection");
    sqlx::query("ANALYZE fleet.obligations")
        .execute(&mut *connection)
        .await
        .expect("statistics refresh");
    let mut transaction = sqlx::Transaction::begin(connection, None)
        .await
        .expect("a transaction to roll back");
    let explain = format!(
        "EXPLAIN (ANALYZE, BUFFERS) {}",
        afd_outbound::test_util::CLAIM_HEADS
    );
    let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(explain))
        .bind(8_i64)
        .bind(mint_id())
        .bind(obligation::CLAIM_TTL)
        .fetch_all(&mut *transaction)
        .await
        .expect("the claim plans");
    transaction
        .rollback()
        .await
        .expect("the plan's claims roll back");
    let plan = plan.join("\n");
    assert!(plan.contains("idx_obligations_open_lane"), "{plan}");
    ledger.cleanup().await;
}
