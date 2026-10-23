//! Candidate plans bound admission reads to one oldest row per eligible fleet.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet::lease::sql::queue::SELECT_CANDIDATES;
use sqlx::{AssertSqlSafe, PgConnection, Row as _};

use crate::support::Fixtures;

#[path = "integration_lease_plan/claim.rs"]
mod claim;
#[path = "integration_lease_plan/gates.rs"]
mod gates;
#[path = "integration_lease_plan/idle.rs"]
mod idle;
#[path = "integration_lease_plan/meter.rs"]
mod meter;
#[path = "integration_lease_plan/prepared.rs"]
mod prepared;

/// Plans every prepared statement once for all binds, as a cached plan does.
const FORCE_GENERIC: &str = "SET LOCAL plan_cache_mode = force_generic_plan";
/// The scan's text prepared with the types sqlx binds it with.
const PREPARE: &str = "PREPARE ledger_candidates \
(text, text, bigint, interval, text, text[], text) AS ";
/// Its plan. The values are placeholders a generic plan never reads.
const EXPLAIN: &str = "EXPLAIN (VERBOSE, COSTS OFF) EXECUTE ledger_candidates \
('active', '01890a5d-ac96-774b-bcce-b302099a80a1', 64, '30 seconds', 'active', '{}', NULL)";
/// Drops the prepared text, which outlives the transaction.
const DEALLOCATE: &str = "DEALLOCATE ledger_candidates";
/// The plan line naming what the scan sorts by.
const SORT_KEY: &str = "Sort Key:";
/// The bind carrying the cursor's partition's ids.
const HELD_IDS: &str = "$6";

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
async fn test_the_held_ids_are_cast_once_per_scan() {
    let fixtures = Fixtures::create().await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    run(&mut connection, "BEGIN").await;
    run(&mut connection, FORCE_GENERIC).await;
    run(&mut connection, &format!("{PREPARE}{SELECT_CANDIDATES}")).await;
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
        !sort_key.contains(HELD_IDS),
        "the cursor's ids are cast for every sorted row: {sort_key}"
    );
}

// Value: protects=one head per fleet as backlog grows; fails_when=the probe reads
// the backlog; why_new=the empty-ledger precheck adds a separate plan branch;
// seam=none; no_claim=constant total discovery cost with many active fleets.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn backlog_growth_does_not_expand_each_fleets_head_probe() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = crate::seed::seeded_parts::<1>(&fixtures).await;
    let mut connection = fixtures.database.acquire().await.expect("database");
    let mut tx = sqlx::Connection::begin(&mut *connection)
        .await
        .expect("isolated backlog");
    let mut previous = 0;
    for size in [2000, 4000, 8000, 16000] {
        let ids: Vec<_> = (previous..size)
            .map(|_| afd_db::test_util::mint_id())
            .collect();
        sqlx::query("INSERT INTO fleet.admissions (id, fleet_id, workspace_id, producer, producer_key, payload_digest, actor, event_type, request_json, event_created_at, created_at, updated_at) SELECT id::uuid,$1::uuid,$2::uuid,'steer',id,'fixture','fixture','chat','{}',now(),now(),now() FROM unnest($3::text[]) id")
            .bind(&fleet).bind(&workspace).bind(&ids).execute(&mut *tx).await.expect("grow backlog");
        let plan = head_plan(&mut tx, runner.as_str()).await;
        let scans = relation_scans(&plan, "admissions");
        assert_eq!(scans.len(), 2, "empty-ledger check and fleet head: {plan}");
        let scan = scans
            .iter()
            .find(|scan| {
                scan["Index Cond"]
                    .as_str()
                    .is_some_and(|s| s.contains("fleet_id"))
            })
            .expect("indexed fleet head");
        assert_eq!(scan["Index Name"], "idx_admissions_unsettled", "{plan}");
        assert_eq!(
            scan["Actual Rows"].as_f64(),
            Some(1.0),
            "backlog {size}: {plan}"
        );
        assert_eq!(
            scan["Actual Loops"].as_f64(),
            Some(1.0),
            "only the placeable fleet: {plan}"
        );
        assert!(
            scan["Index Cond"]
                .as_str()
                .is_some_and(|s| s.contains("fleet_id")),
            "{plan}"
        );
        previous = size;
    }
    tx.rollback().await.expect("remove backlog");
    drop(connection);
    fixtures.cleanup().await;
}

async fn head_plan(connection: &mut PgConnection, runner: &str) -> serde_json::Value {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, FORMAT JSON) {SELECT_CANDIDATES}"
    )))
    .bind(afd_fleet::lease::sql::FLEET_STATUS_ACTIVE)
    .bind(runner)
    .bind(64_i64)
    .bind(afd_core::timing::RUNNER_OFFLINE_AFTER)
    .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
    .bind(Vec::<String>::new())
    .bind(Option::<&str>::None)
    .fetch_one(connection)
    .await
    .expect("candidate explain")
}

fn relation_scans<'a>(plan: &'a serde_json::Value, relation: &str) -> Vec<&'a serde_json::Value> {
    match plan {
        serde_json::Value::Object(fields) => {
            let mut found = Vec::new();
            if fields
                .get("Relation Name")
                .and_then(serde_json::Value::as_str)
                == Some(relation)
            {
                found.push(plan);
            }
            found.extend(
                fields
                    .values()
                    .flat_map(|child| relation_scans(child, relation)),
            );
            found
        }
        serde_json::Value::Array(children) => children
            .iter()
            .flat_map(|child| relation_scans(child, relation))
            .collect(),
        _ => Vec::new(),
    }
}
