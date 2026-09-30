//! Dimension 2.7: the access statement and the team reads plan as index
//! probes, never a scan.
//!
//! The access statement runs before every workspace request, so its cost is
//! paid everywhere. It joins three tables, and each join has a unique index to
//! take: the workspace by primary key, the user by subject, the membership by
//! account and user. The team reads (an account's pending invitations, what
//! waits for an address, the members, the accounts a person holds, and the
//! accept lock) each have an index too. This asserts the planner takes them
//! with enough rows seeded that a scan would be the wrong choice, under both
//! plans a prepared statement can run: the custom plan of its first
//! executions, and the generic plan it settles into.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_tenant::sql::invite::{LOCK_INVITE, SELECT_PENDING_FOR_EMAIL, SELECT_TENANT_PENDING};
use afd_tenant::sql::member::SELECT_MEMBERS;
use afd_tenant::sql::workspace::{AUTHORIZE_WORKSPACE, SELECT_SUBJECT_ACCOUNTS};
use afd_tenant::workspace::access::{ROLE_MEMBER, ROLE_OWNER};
use sqlx::{AssertSqlSafe, Row as _};

/// Enough rows per table that a sequential scan loses to an index probe.
const SEEDED: i32 = 3000;

/// The tables the statement reads, as a plan names them.
const READ: [&str; 3] = ["workspaces", "users", "memberships"];

/// The two plan modes a prepared statement runs under.
const PLAN_MODES: [&str; 2] = ["force_custom_plan", "force_generic_plan"];

/// One account per row, each with a user, a membership, a workspace and a
/// pending invitation, all
/// under identifiers sharing a random per-run prefix, and tenants named for
/// it so cleanup removes exactly these and nothing another suite seeded.
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
           SELECT (stem || '000000000001')::uuid, 'plan-' || $1, 1, 1 FROM n \
         ), people AS ( \
           INSERT INTO core.users \
             (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
           SELECT (stem || '000000000002')::uuid, (stem || '000000000001')::uuid, \
                  'user_plan_' || $1 || '_' || g, 'plan@example.test', NULL, 1, 1 FROM n \
         ), memberships AS ( \
           INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
           SELECT (stem || '000000000003')::uuid, (stem || '000000000001')::uuid, \
                  (stem || '000000000002')::uuid, $3, 1 FROM n \
         ), workspaces AS ( \
           INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
           SELECT (stem || '000000000004')::uuid, (stem || '000000000001')::uuid, \
                  'plan-' || g, 'plan', 1 FROM n \
         ) \
         INSERT INTO core.invites \
           (id, tenant_id, email, role, invited_by, expires_at, email_attempts, \
            created_at, updated_at) \
         SELECT (stem || '000000000005')::uuid, (stem || '000000000001')::uuid, \
                'guest-' || $1 || '-' || g || '@plan.test', $3, \
                (stem || '000000000002')::uuid, 9000000000000, 0, 1, 1 FROM n",
    )
    .bind(prefix)
    .bind(SEEDED)
    .bind(ROLE_MEMBER)
    .execute(&mut *connection)
    .await
    .expect("the plan rows seed");
    sqlx::query("ANALYZE core.workspaces, core.users, core.memberships, core.invites")
        .execute(&mut *connection)
        .await
        .expect("the planner's statistics refresh");
}

/// One statement to explain: its parameter types and the arguments for row one.
struct Probe<'a> {
    statement: &'a str,
    types: &'a str,
    arguments: String,
}

/// The plan text for `probe` under `mode`.
///
/// Through `PREPARE` and `EXPLAIN EXECUTE`, the path a prepared statement
/// takes: a bare `EXPLAIN` plans directly and never consults the plan cache,
/// so `plan_cache_mode` would change nothing it shows. Simple protocol, since
/// neither is a statement the extended protocol prepares.
async fn plan(database: &Db, prefix: &str, probe: &Probe<'_>, mode: &str) -> Vec<String> {
    let mut connection = database.acquire().await.expect("an API connection");
    let name = format!("plan_{prefix}");
    let Probe {
        statement,
        types,
        arguments,
    } = probe;
    let script = format!(
        "SET plan_cache_mode = {mode}; \
         PREPARE {name}({types}) AS {statement}; \
         EXPLAIN EXECUTE {name}({arguments});"
    );
    // `AssertSqlSafe`: neither statement takes a bind parameter, and every
    // interpolated value is a constant here or hex drawn from a minted id.
    let rows = sqlx::raw_sql(AssertSqlSafe(script))
        .fetch_all(&mut *connection)
        .await
        .expect("the statement explains");
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

/// A lane seeded for one run, under a prefix no other run shares.
struct Seeded {
    lane: TestDatabase,
    database: Db,
    prefix: String,
}

impl Seeded {
    async fn open() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        // Eight hex digits from the random half of a fresh identifier: unique
        // per run, and unlike `mint_id`, whose ids all begin `01900000`.
        let fresh = Entropy::new()
            .uuid7(afd_core::clock::now())
            .expect("the host draws entropy");
        let prefix: String = fresh.as_str().chars().rev().take(8).collect();
        seed(&database, &prefix).await;
        Self {
            lane,
            database,
            prefix,
        }
    }

    /// Row one's identifier for the seeded row kind `suffix`.
    fn row(&self, suffix: u8) -> String {
        format!("'{}-0001-7000-8000-00000000000{suffix}'", self.prefix)
    }

    /// Asserts `probe` reads every one of `tables` through an index, never a
    /// scan, under both plan modes.
    async fn assert_probes(&self, label: &str, probe: &Probe<'_>, tables: &[&str]) {
        for mode in PLAN_MODES {
            let lines = plan(&self.database, &self.prefix, probe, mode).await;
            let text = lines.join("\n");
            for table in tables {
                assert!(
                    !lines
                        .iter()
                        .any(|line| line.contains("Seq Scan") && line.contains(table)),
                    "{label}, {mode}: {table} is scanned, not probed:\n{text}"
                );
                assert!(
                    lines
                        .iter()
                        .any(|line| line.contains("Index") && line.contains(table)),
                    "{label}, {mode}: {table} is not read through an index:\n{text}"
                );
            }
        }
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        // By the run's own tenant name, never by an id pattern: fixture ids from
        // `mint_id` all start alike, so a prefix match here once removed every
        // tenant the concurrent suites had seeded.
        sqlx::query("DELETE FROM core.tenants WHERE name = 'plan-' || $1")
            .bind(&self.prefix)
            .execute(&mut *connection)
            .await
            .expect("the plan rows clean up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_access_check_plans_as_index_probes() {
    let seeded = Seeded::open().await;
    let probe = Probe {
        statement: AUTHORIZE_WORKSPACE,
        types: "text, text, text",
        arguments: format!("{}, 'user_plan_{}_1', NULL", seeded.row(4), seeded.prefix),
    };
    seeded
        .assert_probes("the access check", &probe, &READ)
        .await;
    seeded.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_team_reads_plan_as_index_probes() {
    let seeded = Seeded::open().await;
    let owner = format!("'{ROLE_OWNER}'");
    let subject = format!("'user_plan_{}_1'", seeded.prefix);
    let address = format!("'guest-{}-1@plan.test'", seeded.prefix);
    let reads = [
        (
            "pending invitations",
            SELECT_TENANT_PENDING,
            "text, bigint",
            format!("{}, 2", seeded.row(1)),
            &["invites"][..],
        ),
        (
            "waiting for an address",
            SELECT_PENDING_FOR_EMAIL,
            "text, bigint, text",
            format!("{address}, 2, {owner}"),
            &["invites", "tenants", "memberships"][..],
        ),
        (
            "members",
            SELECT_MEMBERS,
            "text",
            seeded.row(1),
            &["memberships", "users"][..],
        ),
        (
            "accounts held",
            SELECT_SUBJECT_ACCOUNTS,
            "text, text",
            format!("{subject}, {owner}"),
            &["users", "memberships", "tenants"][..],
        ),
        (
            "the accept lock",
            LOCK_INVITE,
            "text",
            seeded.row(5),
            &["invites"][..],
        ),
    ];
    for (label, statement, types, arguments, tables) in reads {
        let probe = Probe {
            statement,
            types,
            arguments,
        };
        seeded.assert_probes(label, &probe, tables).await;
    }
    seeded.cleanup().await;
}
