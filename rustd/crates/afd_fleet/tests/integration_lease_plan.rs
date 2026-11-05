//! The claim's plan drives from the readiness index and reads one head
//! admission per fleet it considers, however large the backlog or the estate.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::fmt::Write as _;

use afd_fleet::lease::sql::queue::CLAIM_HEAD;
use sqlx::{AssertSqlSafe, PgConnection, Row as _};

use crate::support::Fixtures;

#[path = "integration_lease_plan/claim.rs"]
mod claim;
#[path = "integration_lease_plan/meter.rs"]
mod meter;
#[path = "integration_lease_plan/ready.rs"]
mod ready;

/// Plans every prepared statement once for all binds, as a cached plan does.
const FORCE_GENERIC: &str = "SET LOCAL plan_cache_mode = force_generic_plan";
/// The claim's text prepared with the types sqlx binds it with.
const PREPARE: &str = "PREPARE ledger_claim \
(text, text[], interval, interval, text, text, text, text, text, text, text) AS ";
/// Its plan, not run. The values are placeholders a generic plan never reads.
const EXPLAIN: &str = "EXPLAIN (VERBOSE, COSTS OFF) EXECUTE ledger_claim \
('01890a5d-ac96-774b-bcce-b302099a80a1', '{}', '30 seconds', '90 seconds', 'active', \
 'active', 'expired', 'active', 'received', 'steer', NULL)";
/// Drops the prepared text, which outlives the transaction.
const DEALLOCATE: &str = "DEALLOCATE ledger_claim";
/// The plan line naming what the claim sorts by.
const SORT_KEY: &str = "Sort Key:";
/// The bind carrying the runner's held fleets.
const HELD_IDS: &str = "$2";

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
async fn test_the_held_ids_are_cast_once_per_claim() {
    let fixtures = Fixtures::create().await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    run(&mut connection, "BEGIN").await;
    run(&mut connection, FORCE_GENERIC).await;
    run(&mut connection, &format!("{PREPARE}{CLAIM_HEAD}")).await;
    let plan: Vec<String> = sqlx::raw_sql(AssertSqlSafe(EXPLAIN))
        .fetch_all(&mut *connection)
        .await
        .expect("the claim explains")
        .iter()
        .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
        .collect();
    run(&mut connection, DEALLOCATE).await;
    run(&mut connection, "ROLLBACK").await;

    let sort_key = plan
        .iter()
        .find(|line| line.contains(SORT_KEY))
        .expect("the claim sorts");
    assert!(
        !sort_key.contains(HELD_IDS),
        "the held ids are cast for every sorted row: {sort_key}"
    );
}

// Value: protects=one head per fleet as backlog grows; fails_when=the claim
// reads the backlog; why_new=the claim joins the head through a lateral probe;
// seam=none; no_claim=constant total claim cost with many active fleets.
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
        // A plan claim rests on current statistics (SQL-PLAN); the rows are
        // this transaction's own, which `ANALYZE` counts.
        for table in ["fleet.admissions", "runner.fleet_slots"] {
            run(&mut tx, &format!("ANALYZE {table}")).await;
        }
        // The plan runs the claim, so each reading is rolled back to keep the
        // fleet claimable for the next size.
        run(&mut tx, "SAVEPOINT head_probe").await;
        let plan = claim_plan(&mut tx, runner.as_str()).await;
        run(&mut tx, "ROLLBACK TO SAVEPOINT head_probe").await;
        let head = relation_scans(&plan, "admissions")
            .into_iter()
            .find(|scan| scan["Index Name"] == "idx_admissions_unsettled")
            .unwrap_or_else(|| panic!("the head comes off the unsettled index: {plan}"));
        assert_eq!(
            head["Actual Rows"].as_f64(),
            Some(1.0),
            "backlog {size}: {plan}"
        );
        assert_eq!(
            head["Actual Loops"].as_f64(),
            Some(1.0),
            "only the one ready fleet:\n{}",
            tree(&plan, 0)
        );
        for scan in read_scans(&plan, "admissions") {
            let read = scan["Actual Rows"].as_f64().unwrap_or(f64::MAX)
                * scan["Actual Loops"].as_f64().unwrap_or(f64::MAX);
            assert!(
                read <= 1.0,
                "backlog {size}: no admissions scan reads past one row:\n{}",
                tree(&plan, 0)
            );
        }
        previous = size;
    }
    tx.rollback().await.expect("remove backlog");
    drop(connection);
    fixtures.cleanup().await;
}

/// The claim, run and explained: its writes stand until the caller rolls back.
async fn claim_plan(connection: &mut PgConnection, runner: &str) -> serde_json::Value {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {CLAIM_HEAD}"
    )))
    .bind(runner)
    .bind(Vec::<String>::new())
    .bind(afd_core::timing::LEASE_TTL)
    .bind(afd_core::timing::RUNNER_OFFLINE_AFTER)
    .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
    .bind(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE)
    .bind(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
    .bind(afd_fleet::lease::sql::FLEET_STATUS_ACTIVE)
    .bind(afd_core::event::status::RECEIVED)
    .bind(afd_admission::Producer::Steer.as_str())
    .bind(None::<&str>)
    .fetch_one(connection)
    .await
    .expect("claim explain")
}

/// Every plan node that READS `relation`: scans, not the writes that name it.
fn read_scans<'a>(plan: &'a serde_json::Value, relation: &str) -> Vec<&'a serde_json::Value> {
    relation_scans(plan, relation)
        .into_iter()
        .filter(|node| {
            node["Node Type"]
                .as_str()
                .is_some_and(|kind| kind.ends_with("Scan"))
        })
        .collect()
}

/// The plan as one line per node: type, relation, index, estimated and actual
/// rows, and loops.
fn tree(plan: &serde_json::Value, depth: usize) -> String {
    match plan {
        serde_json::Value::Object(fields) => {
            let mut out = String::new();
            if let Some(kind) = fields.get("Node Type").and_then(serde_json::Value::as_str) {
                let field = |name: &str| {
                    fields
                        .get(name)
                        .map(ToString::to_string)
                        .unwrap_or_default()
                };
                let _written = writeln!(
                    out,
                    "{}{kind} {} {} {} est={} rows={} loops={}",
                    "  ".repeat(depth),
                    field("Subplan Name"),
                    field("Relation Name"),
                    field("Index Name"),
                    field("Plan Rows"),
                    field("Actual Rows"),
                    field("Actual Loops"),
                );
            }
            for (key, child) in fields {
                if key == "Plans" || key == "Plan" {
                    out.push_str(&tree(child, depth + 1));
                }
            }
            out
        }
        serde_json::Value::Array(children) => {
            children.iter().map(|child| tree(child, depth)).collect()
        }
        _ => String::new(),
    }
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
