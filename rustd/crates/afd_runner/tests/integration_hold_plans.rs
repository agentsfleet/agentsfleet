//! The heartbeat's hold reconcile reaches its slots through the partial index
//! `schema/932` builds for it, under the generic plan a prepared statement
//! settles on.
//!
//! sqlx prepares `CLEAR_DROPPED_HOLDS` once per connection, and Postgres may
//! then plan it once for every runner. A partial index serves that plan only
//! when the statement's own predicate implies the index's whatever the binds
//! are, which `held_until IS NOT NULL`, a literal in both, does. An index
//! whose predicate drifted from the statement's would leave every beat of
//! every runner reading each slot that runner was ever last on.
//!
//! The plan is asked of a database of this test's own, migrated from
//! `schema/`, seeded with mostly unheld slots in a transaction that rolls
//! back, and analysed, so its statistics describe the spread and nothing else:
//! the shape `afd_events`' `integration_list_plans.rs` set. That suite asks
//! `EXPLAIN (GENERIC_PLAN)`, which cannot type `$3 - $4`; here the text is
//! prepared with the types sqlx binds and explained with generic plans forced.
//! No planner switch is touched, so the index wins on cost or not at all.
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
use afd_runner::sql::holds::CLEAR_DROPPED_HOLDS;
use sqlx::{AssertSqlSafe, PgConnection, Row as _};

/// The index under test.
const HELD_INDEX: &str = "idx_runner_affinity_last_runner_id_held";

/// The spreads the plan is asked against, as `(runners, slots per runner, one
/// slot in this many held)`: a small fleet and a larger one, because the
/// choice between an index and a scan turns on how many rows are expected.
const SPREADS: [(i32, i32, i32); 2] = [(8, 125, 20), (40, 250, 50)];

/// The instant every seeded row is stamped at.
const SEEDED_AT: i64 = 1_700_000_000_000;

/// The one tenant every seeded row belongs to.
const SEED_TENANT: &str = "INSERT INTO core.tenants (id, name, created_at, updated_at)
VALUES ($1::uuid, 'hold-plan', $2, $2)";

/// Runners, a fleet per slot, and the slots, dealt round the runners, every
/// `$5`th held: `$1` tenant, `$2` instant, `$3` runners, `$4` slots each.
const SEED_SPREAD: &str = "
WITH workspace AS (
    INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
    VALUES (uuidv7(), $1::uuid, 'hold-plan', 'hold-plan', $2)
    RETURNING id
), runners AS (
    INSERT INTO fleet.runners
      (id, host_id, token_hash, sandbox_tier, admin_state, labels,
       last_seen_at, created_at, updated_at)
    SELECT uuidv7(), 'hold-plan-' || r, 'hold-plan-' || r, 'dev_none', 'active',
           '[]'::jsonb, $2, $2, $2
      FROM generate_series(1, $3) r
    RETURNING id
), ranked AS (
    SELECT id, row_number() OVER (ORDER BY id) - 1 AS rank FROM runners
), fleets AS (
    INSERT INTO core.fleets
      (id, workspace_id, tenant_id, name, source_markdown, config_json, status,
       created_at, updated_at)
    SELECT uuidv7(), workspace.id, $1::uuid, 'hold-plan-' || f, '# probe',
           '{}'::jsonb, 'active', $2, $2
      FROM workspace, generate_series(1, $3 * $4) f
    RETURNING id
), slots AS (
    SELECT id, row_number() OVER (ORDER BY id) - 1 AS n FROM fleets
)
INSERT INTO fleet.runner_affinity
  (fleet_id, last_runner_id, fencing_seq, leased_until, metered_input_tokens,
   metered_cached_tokens, metered_output_tokens, last_metered_at, created_at,
   updated_at, held_until)
SELECT slots.id, ranked.id, 1, $2, 0, 0, 0, $2, $2, $2,
       CASE WHEN slots.n % $5 = 0 THEN $2 + 1 END
  FROM slots JOIN ranked ON ranked.rank = slots.n % $3";

/// Refreshes the statistics the plan is costed from.
const ANALYZE_SLOTS: &str = "ANALYZE fleet.runner_affinity";

/// Plans every prepared statement once for all binds, as a cached plan does.
const FORCE_GENERIC: &str = "SET LOCAL plan_cache_mode = force_generic_plan";

/// The reconcile's text prepared with the types sqlx binds it with: the
/// runner and the listed fleets as text, the instant and interval as `i64`.
const PREPARE_CLEAR: &str = "PREPARE clear_dropped_holds (text, text[], bigint, bigint) AS ";

/// Its plan. The values are placeholders a generic plan never reads.
const EXPLAIN_CLEAR: &str = "EXPLAIN EXECUTE clear_dropped_holds \
('01890a5d-ac96-774b-bcce-b302099a80a1', '{}', 0, 0)";

/// Drops the prepared text, which outlives the transaction.
const DEALLOCATE_CLEAR: &str = "DEALLOCATE clear_dropped_holds";

/// Runs one statement that returns nothing worth reading. Every text is this
/// file's own constant, never input.
async fn run(connection: &mut PgConnection, statement: &str) {
    sqlx::raw_sql(AssertSqlSafe(statement.to_owned()))
        .execute(&mut *connection)
        .await
        .unwrap_or_else(|failure| panic!("{statement}: {failure}"));
}

/// A database of this test's own, migrated from `schema/`, and its tenant.
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
        .bind(SEEDED_AT)
        .execute(&mut *connection)
        .await
        .expect("the tenant must insert");
    drop(connection);
    (database, db, tenant)
}

/// The reconcile's generic plan against one spread, which rolls back.
async fn generic_plan(
    db: &Db,
    tenant: &str,
    (runners, each, held_one_in): (i32, i32, i32),
) -> String {
    let mut connection = db.acquire().await.expect("a private connection");
    run(&mut connection, "BEGIN").await;
    sqlx::query(SEED_SPREAD)
        .bind(tenant)
        .bind(SEEDED_AT)
        .bind(runners)
        .bind(each)
        .bind(held_one_in)
        .execute(&mut *connection)
        .await
        .expect("the spread must insert");
    run(&mut connection, ANALYZE_SLOTS).await;
    run(&mut connection, FORCE_GENERIC).await;
    run(
        &mut connection,
        &format!("{PREPARE_CLEAR}{CLEAR_DROPPED_HOLDS}"),
    )
    .await;
    let plan = sqlx::raw_sql(AssertSqlSafe(EXPLAIN_CLEAR))
        .fetch_all(&mut *connection)
        .await
        .expect("the reconcile must be explainable")
        .iter()
        .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
        .collect::<Vec<_>>()
        .join("\n");
    run(&mut connection, DEALLOCATE_CLEAR).await;
    run(&mut connection, "ROLLBACK").await;
    plan
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_hold_reconcile_clears_through_the_held_index() {
    let (database, db, tenant) = private_database().await;

    for spread in SPREADS {
        let plan = generic_plan(&db, &tenant, spread).await;
        assert!(
            plan.contains("$1"),
            "not a generic plan at {spread:?}:\n{plan}"
        );
        assert!(
            plan.contains(HELD_INDEX),
            "the reconcile does not use {HELD_INDEX} at {spread:?}:\n{plan}"
        );
    }
    db.close().await;
    database.cleanup().await;
}
