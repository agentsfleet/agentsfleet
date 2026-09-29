//! Dimension 4.2 — the fleet, workspace and thread reads keep their scope,
//! cursor and `since` as index conditions under a generic plan.
//!
//! sqlx prepares each text once per connection, and Postgres may then plan it
//! once for every value. A bound that plan cannot decide — anything behind
//! `IS NULL OR` — becomes a filter, and a deep page then reads every newer row
//! to reach its cursor. Two halves prove the split texts avoid that:
//!
//! - the pages run through `History` a few rows at a time, so every text runs
//!   with the binds `History` hands it, and each walk must return every row
//!   once in keyset order, including two events in one millisecond;
//! - `EXPLAIN (GENERIC_PLAN)` of every text `History` runs must show what
//!   `expected.rs` states for it, at two spreads of data.
//!
//! The plans are asked of a database of this test's own, migrated from
//! `schema/`, so its statistics describe the spread and nothing else. The
//! shared lane's table is one fleet per workspace from every other suite's
//! fixtures, and its row count grows through a run: against it the fleet and
//! workspace indexes tie for a fleet read, and the planner's pick is noise.
//! Each spread holds several fleets per workspace, is seeded in a transaction
//! that rolls back, and is analysed, which also fills the dependency
//! statistics `schema/922` declares; without them a fleet-scoped plan sorts.
//! No planner switch is touched: a priced-out alternative would prove only
//! that SOME index is usable.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, Migrator};
use afd_events::READ_TEXTS;
use sqlx::{AssertSqlSafe, PgConnection, Row as _};

use crate::support::EventsLane;

#[path = "integration_list_plans/expected.rs"]
mod expected;
#[path = "integration_list_plans/walk.rs"]
mod walk;

/// The decoy spreads the plans are asked against, as `(workspaces, fleets per
/// workspace, events per fleet)`: a small history and a larger one, because
/// the planner's choice between walking an index and sorting turns on how
/// many rows it expects.
const SPREADS: [(i32, i32, i32); 2] = [(20, 5, 40), (50, 10, 200)];

/// The first decoy event's timestamp, and the tenant's.
const DECOY_MS: i64 = 1_700_000_000_000;

/// The one tenant every decoy workspace belongs to.
const SEED_TENANT: &str = "INSERT INTO core.tenants (id, name, created_at, updated_at)
VALUES ($1::uuid, 'plan-probe', $2, $2)";

/// Decoy workspaces, their fleets and their events, in one statement.
const SEED_SPREAD: &str = "
WITH workspaces AS (
    INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
    SELECT uuidv7(), $1::uuid, 'plan-probe-' || w, 'plan-probe', $2
      FROM generate_series(1, $3) w
    RETURNING id
), fleets AS (
    INSERT INTO core.fleets
      (id, workspace_id, tenant_id, name, source_markdown, config_json, status,
       created_at, updated_at)
    SELECT uuidv7(), workspaces.id, $1::uuid, 'plan-probe-' || f, '# probe',
           '{}'::jsonb, 'active', $2, $2
      FROM workspaces, generate_series(1, $4) f
    RETURNING id, workspace_id
)
INSERT INTO core.fleet_events
  (fleet_id, workspace_id, event_id, actor, event_type, status, request_json,
   created_at, updated_at)
SELECT fleets.id, fleets.workspace_id, 'plan-probe-' || e, 'steer:api', 'chat',
       'completed', '{}'::jsonb, $2 + e, $2 + e
  FROM fleets, generate_series(1, $5) e";

/// Refreshes the statistics every plan here is costed from, the dependency
/// statistics among them.
const ANALYZE_EVENTS: &str = "ANALYZE core.fleet_events";

/// Runs one statement that returns nothing worth reading.
async fn run(connection: &mut PgConnection, statement: &'static str) {
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .unwrap_or_else(|failure| panic!("{statement}: {failure}"));
}

/// The generic plan of `text`, one line per plan row.
async fn explain_generic(connection: &mut PgConnection, text: &str) -> String {
    // The simple protocol, because the text's placeholders stay unbound: a
    // prepared EXPLAIN would ask for values. The text is this crate's own
    // constant with a keyword in front of it, never input.
    sqlx::raw_sql(AssertSqlSafe(format!("EXPLAIN (GENERIC_PLAN) {text}")))
        .fetch_all(&mut *connection)
        .await
        .expect("every read text must be explainable")
        .iter()
        .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A database of this test's own, migrated from `schema/`, and the tenant
/// its decoys belong to.
async fn private_database() -> (TestDatabase, Db, String) {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&db)
        .await
        .expect("the private database must migrate");
    let tenant = mint_id();
    let mut connection = db.acquire().await.expect("a private connection");
    sqlx::query(SEED_TENANT)
        .bind(tenant.as_str())
        .bind(DECOY_MS)
        .execute(&mut *connection)
        .await
        .expect("the decoy tenant must insert");
    drop(connection);
    (database, db, tenant)
}

/// Every text's generic plan against one decoy spread, which rolls back.
async fn generic_plans(
    db: &Db,
    tenant: &str,
    (workspaces, fleets, events): (i32, i32, i32),
) -> Vec<(&'static str, String)> {
    let mut connection = db.acquire().await.expect("a private connection");
    run(&mut connection, "BEGIN").await;
    sqlx::query(SEED_SPREAD)
        .bind(tenant)
        .bind(DECOY_MS)
        .bind(workspaces)
        .bind(fleets)
        .bind(events)
        .execute(&mut *connection)
        .await
        .expect("the decoy spread must insert");
    run(&mut connection, ANALYZE_EVENTS).await;

    let mut plans = Vec::new();
    for (name, text) in READ_TEXTS {
        plans.push((name, explain_generic(&mut connection, text).await));
    }
    run(&mut connection, "ROLLBACK").await;
    plans
}

/// The plan's lines that start with `label`, trimmed.
fn labelled<'plan>(plan: &'plan str, label: &str) -> Vec<&'plan str> {
    plan.lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with(label))
        .collect()
}

/// Every one of `bounds` is an index condition of some scan, and none is left
/// to a filter.
fn assert_bounds_are_index_conditions(name: &str, plan: &str, bounds: &[&str]) {
    let conditions = labelled(plan, "Index Cond:");
    let filters = labelled(plan, "Filter:");
    for bound in bounds {
        assert!(
            conditions.iter().any(|line| line.contains(bound)),
            "{name}: `{bound}` is not an index condition:\n{plan}"
        );
        assert!(
            !filters.iter().any(|line| line.contains(bound)),
            "{name}: `{bound}` is filtered row by row:\n{plan}"
        );
    }
}

/// `plan` walks `index` in order with every one of `bounds` on it, and sorts
/// nothing — the plan that lets `LIMIT` stop the read at one page.
fn assert_walks_the_index(name: &str, plan: &str, index: &str, bounds: &[&str]) {
    let mut lines = plan.lines().skip_while(|line| !line.contains(index));
    let walk = lines.next().unwrap_or_default();
    let condition = lines.next().unwrap_or_default();
    assert!(
        walk.contains(&format!("Index Scan using {index} on fleet_events "))
            && bounds.iter().all(|bound| condition.contains(bound))
            && !plan.contains("Sort"),
        "{name} does not walk {index} in order with its bounds:\n{plan}"
    );
}

/// Dimension 4.2.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_event_list_plans_use_the_index() {
    let lane = EventsLane::open().await;
    let seeded = walk::seed_walk(&lane).await;
    walk::assert_walks(&lane, &seeded).await;

    lane.cleanup().await;

    let (database, db, tenant) = private_database().await;
    for spread in SPREADS {
        let plans = generic_plans(&db, &tenant, spread).await;
        assert_eq!(
            plans.len(),
            expected::EXPECTED.len(),
            "a text has no expectation"
        );
        for (name, ordered, bounds) in expected::EXPECTED {
            let plan = plans
                .iter()
                .find(|(planned, _)| *planned == name)
                .map_or_else(
                    || panic!("{name} is not a text History runs"),
                    |(_, plan)| plan.as_str(),
                );
            let name = format!("{name} at spread {spread:?}");
            assert_bounds_are_index_conditions(&name, plan, bounds);
            if let Some(index) = ordered {
                assert_walks_the_index(&name, plan, index, bounds);
            }
        }
    }
    db.close().await;
    database.cleanup().await;
}
