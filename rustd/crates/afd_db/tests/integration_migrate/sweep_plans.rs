//! The two hot sweeps read their own index, under the generic plan a prepared
//! statement settles on.
//!
//! Each statement is prepared with the types sqlx binds and explained with
//! sequential scans priced out, so a plan that names no index of the sweep's
//! shape means no index serves it. The tables are empty: what is pinned is
//! that the index exists and matches, not the cost the planner gives it.

use sqlx::AssertSqlSafe;

use super::*;

/// The lapse sweep (`afd_authoring` maintenance): `$1` status, `$2` limit.
const ATTEMPT_CANDIDATES: &str =
    include_str!("../../../afd_authoring/src/maintenance/sql/candidates.sql");

/// The metering sweep (`afd_fleet` lease): `$1` status, `$2` grace, `$3` limit.
const UNBILLED_LEASES: &str =
    include_str!("../../../afd_fleet/src/lease/sql/select_unbilled_expired_leases.sql");

/// The plan of `statement`, prepared as `types` and executed with `binds`, a
/// generic plan with sequential scans priced out.
async fn plan(db: &afd_db::Db, statement: &str, types: &str, binds: &str) -> String {
    let mut connection = db.acquire().await.unwrap();
    let setup = [
        "BEGIN".to_owned(),
        "SET LOCAL plan_cache_mode = force_generic_plan".to_owned(),
        "SET LOCAL enable_seqscan = off".to_owned(),
        format!("PREPARE sweep ({types}) AS {statement}"),
    ];
    for step in setup {
        sqlx::raw_sql(AssertSqlSafe(step))
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    let lines: Vec<String> = sqlx::raw_sql(AssertSqlSafe(format!(
        "EXPLAIN (COSTS OFF) EXECUTE sweep ({binds})"
    )))
    .fetch_all(&mut *connection)
    .await
    .expect("the sweep explains")
    .iter()
    .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
    .collect();
    sqlx::raw_sql(AssertSqlSafe("DEALLOCATE sweep; ROLLBACK"))
        .execute(&mut *connection)
        .await
        .unwrap();
    lines.join("\n")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_attempt_lapse_sweep_reads_its_deadline_index() {
    let (database, db) = migrated().await;
    // The placeholders are never read: a generic plan is bind-independent.
    let plan = plan(&db, ATTEMPT_CANDIDATES, "text, bigint", "'s', 1").await;
    assert!(
        plan.contains("idx_attempts_status_deadline_id"),
        "the lapse sweep reads no deadline index:\n{plan}"
    );
    assert!(
        !plan.contains("Sort"),
        "the index order is the sweep's order, so nothing sorts:\n{plan}"
    );
    db.close().await;
    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_metering_sweep_reads_the_unbilled_lease_index() {
    let (database, db) = migrated().await;
    let plan = plan(
        &db,
        UNBILLED_LEASES,
        "text, interval, bigint",
        "'s', '1 second', 1",
    )
    .await;
    assert!(
        plan.contains("idx_leases_lease_expires_at_unbilled"),
        "the metering sweep reads no unbilled-lease index:\n{plan}"
    );
    db.close().await;
    database.cleanup().await;
}
