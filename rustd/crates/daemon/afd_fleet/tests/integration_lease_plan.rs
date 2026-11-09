//! Candidate plans bound admission reads to one oldest row per eligible fleet.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_fleet::lease::sql::queue::SELECT_CANDIDATES;
use sqlx::{AssertSqlSafe, PgConnection};

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
/// Runs one statement that returns nothing worth reading. Every text is this
/// file's own constant or the scan's, never input.
async fn run(connection: &mut PgConnection, statement: &str) {
    sqlx::raw_sql(AssertSqlSafe(statement.to_owned()))
        .execute(&mut *connection)
        .await
        .unwrap_or_else(|failure| panic!("{statement}: {failure}"));
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
        sqlx::query("INSERT INTO fleet.admissions (id, fleet_id, workspace_id, producer, producer_key, payload_digest, actor, event_type, request_json, event_created_at, created_at, updated_at) SELECT id,$1,$2,'steer',id,'fixture','fixture','chat','{}',now(),now(),now() FROM unnest($3) id")
            .bind(&fleet).bind(&workspace).bind(&ids).execute(&mut *tx).await.expect("grow backlog");
        let plan = head_plan(&mut tx, &runner).await;
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

async fn head_plan(connection: &mut PgConnection, runner: &Uuid7) -> serde_json::Value {
    sqlx::query_scalar(AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, FORMAT JSON) {SELECT_CANDIDATES}"
    )))
    .bind(afd_fleet::lease::sql::FLEET_STATUS_ACTIVE)
    .bind(runner)
    .bind(64_i64)
    .bind(af_core::timing::RUNNER_OFFLINE_AFTER)
    .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
    .bind(Vec::<Uuid7>::new())
    .bind(Option::<Uuid7>::None)
    .bind(af_wire::task::CAPABILITY)
    .bind(af_wire::task::TaskState::Ready.as_str())
    .bind(af_wire::task::TaskState::Running.as_str())
    .bind(afd_fleet::task::DECISION_POLICY_PATH)
    .bind(false)
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
