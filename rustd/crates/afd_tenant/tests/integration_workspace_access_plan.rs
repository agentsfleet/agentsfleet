//! Dimension 2.7: the access statement plans as index probes, never a scan.
//!
//! It runs before every workspace request, so its cost is paid everywhere. It
//! joins three tables, and each join has a unique index to take: the workspace
//! by primary key, the user by subject, the membership by account and user.
//! This asserts the planner takes them with enough rows seeded that a scan
//! would be the wrong choice, under both plans a prepared statement can run:
//! the custom plan of its first executions, and the generic plan it settles
//! into.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::sql::workspace::AUTHORIZE_WORKSPACE;
use afd_tenant::workspace::access::ROLE_MEMBER;
use sqlx::{AssertSqlSafe, Row as _};

/// Enough rows per table that a sequential scan loses to an index probe.
const SEEDED: i32 = 3000;

/// The tables the statement reads, as a plan names them.
const READ: [&str; 3] = ["workspaces", "users", "memberships"];

/// The two plan modes a prepared statement runs under.
const PLAN_MODES: [&str; 2] = ["force_custom_plan", "force_generic_plan"];

/// One account per row, each with a user, a membership and a workspace, all
/// under identifiers sharing a per-run prefix so cleanup finds exactly them.
async fn seed(database: &Db, prefix: &str) {
    let mut connection = database.acquire().await.expect("an API connection");
    // A UUIDv7 shape per row: the per-run prefix, the row number, and the
    // version nibble the tables' CHECKs demand.
    sqlx::query(
        "WITH n AS ( \
           SELECT g, $1 || '-' || lpad(to_hex(g), 4, '0') || '-7000-8000-' AS stem \
           FROM generate_series(1, $2) g \
         ), tenants AS ( \
           INSERT INTO core.tenants (id, name, created_at, updated_at) \
           SELECT (stem || '000000000001')::uuid, 'plan', 1, 1 FROM n \
         ), people AS ( \
           INSERT INTO core.users \
             (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
           SELECT (stem || '000000000002')::uuid, (stem || '000000000001')::uuid, \
                  'user_plan_' || $1 || '_' || g, 'plan@example.test', NULL, 1, 1 FROM n \
         ), memberships AS ( \
           INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
           SELECT (stem || '000000000003')::uuid, (stem || '000000000001')::uuid, \
                  (stem || '000000000002')::uuid, $3, 1 FROM n \
         ) \
         INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
         SELECT (stem || '000000000004')::uuid, (stem || '000000000001')::uuid, \
                'plan-' || g, 'plan', 1 FROM n",
    )
    .bind(prefix)
    .bind(SEEDED)
    .bind(ROLE_MEMBER)
    .execute(&mut *connection)
    .await
    .expect("the plan rows seed");
    sqlx::query("ANALYZE core.workspaces, core.users, core.memberships")
        .execute(&mut *connection)
        .await
        .expect("the planner's statistics refresh");
}

/// The plan text for the access statement under `mode`, run for row one.
///
/// Through `PREPARE` and `EXPLAIN EXECUTE`, the path a prepared statement
/// takes: a bare `EXPLAIN` plans directly and never consults the plan cache,
/// so `plan_cache_mode` would change nothing it shows. Simple protocol, since
/// neither is a statement the extended protocol prepares.
async fn plan(database: &Db, prefix: &str, mode: &str) -> Vec<String> {
    let mut connection = database.acquire().await.expect("an API connection");
    let name = format!("access_plan_{prefix}");
    let script = format!(
        "SET plan_cache_mode = {mode}; \
         PREPARE {name}(text, text, text) AS {AUTHORIZE_WORKSPACE}; \
         EXPLAIN EXECUTE {name}('{prefix}-0001-7000-8000-000000000004', 'user_plan_{prefix}_1', NULL);"
    );
    // `AssertSqlSafe`: neither statement takes a bind parameter, and every
    // interpolated value is a constant here or hex drawn from a minted id.
    let rows = sqlx::raw_sql(AssertSqlSafe(script))
        .fetch_all(&mut *connection)
        .await
        .expect("the access statement explains");
    sqlx::raw_sql(AssertSqlSafe(format!(
        "DEALLOCATE {name}; RESET plan_cache_mode;"
    )))
    .execute(&mut *connection)
    .await
    .expect("the prepared statement and the mode are released");
    rows.iter()
        .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
        .collect()
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_access_check_plans_as_index_probes() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let prefix: String = mint_id()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(8)
        .collect();
    seed(&database, &prefix).await;

    for mode in PLAN_MODES {
        let lines = plan(&database, &prefix, mode).await;
        let text = lines.join("\n");
        for table in READ {
            assert!(
                !lines
                    .iter()
                    .any(|line| line.contains("Seq Scan") && line.contains(table)),
                "{mode}: {table} is scanned, not probed:\n{text}"
            );
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains("Index") && line.contains(table)),
                "{mode}: {table} is not read through an index:\n{text}"
            );
        }
    }

    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query("DELETE FROM core.tenants WHERE id::text LIKE $1 || '-%'")
        .bind(&prefix)
        .execute(&mut *connection)
        .await
        .expect("the plan rows clean up");
    drop(connection);
    drop(database);
    lane.cleanup().await;
}
