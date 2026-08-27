//! Runner lease-history proofs against the migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::env::MapEnv;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::{Db, Migrator};
use afd_fleet_ops::RunnerLeaseHistory;
use afd_wire::operator::{LeaseKind, LeaseOutcome};
use sqlx::AssertSqlSafe;

const LANE_KNOB: &str = "TEST_DATABASE_URL";
const RUNNER_A: &str = "01950000-0000-7000-8000-000000000021";
const WORKSPACE_B: &str = "01950000-0000-7000-8000-000000000003";
const LEASE_1: &str = "01950000-0000-7000-8000-000000000031";
const LEASE_2: &str = "01950000-0000-7000-8000-000000000032";
const LEASE_3: &str = "01950000-0000-7000-8000-000000000033";
const LEASE_4: &str = "01950000-0000-7000-8000-000000000034";
const FOREIGN_LEASE: &str = "01950000-0000-7000-8000-000000000041";

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

const SEED: &str = r"
WITH inserted_tenant AS (
    INSERT INTO core.tenants (id, name, created_at, updated_at)
    VALUES ('01950000-0000-7000-8000-000000000001', 'Operator test', 1, 1)
), inserted_workspaces AS (
    INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
    VALUES
      ('01950000-0000-7000-8000-000000000002', '01950000-0000-7000-8000-000000000001', 'primary', 'test', 2),
      ('01950000-0000-7000-8000-000000000003', '01950000-0000-7000-8000-000000000001', 'secondary', 'test', 3)
), inserted_fleets AS (
    INSERT INTO core.fleets
      (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
    VALUES
      ('01950000-0000-7000-8000-000000000011', '01950000-0000-7000-8000-000000000002', '01950000-0000-7000-8000-000000000001', 'Production', '# prod', '{}'::jsonb, 'active', 10, 10),
      ('01950000-0000-7000-8000-000000000012', '01950000-0000-7000-8000-000000000003', '01950000-0000-7000-8000-000000000001', 'Canary', '# canary', '{}'::jsonb, 'active', 11, 11)
), inserted_runners AS (
    INSERT INTO fleet.runners
      (id, host_id, token_hash, sandbox_tier, admin_state, labels, last_seen_at, created_at, updated_at)
    VALUES
      ('01950000-0000-7000-8000-000000000021', 'host-a', 'hash-a', 'dev_none', 'active', '[]'::jsonb, 20, 20, 20),
      ('01950000-0000-7000-8000-000000000022', 'host-b', 'hash-b', 'dev_none', 'active', '[]'::jsonb, 21, 21, 21)
), inserted_events AS (
    INSERT INTO core.fleet_events
      (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, wall_ms, failure_label, failure_detail, created_at, updated_at)
    VALUES
      ('01950000-0000-7000-8000-000000000011', '01950000-0000-7000-8000-000000000002', 'event-a', 'user:a', 'chat', 'processed', '{}'::jsonb, 999, NULL, NULL, 50, 50),
      ('01950000-0000-7000-8000-000000000012', '01950000-0000-7000-8000-000000000003', 'event-b', 'user:b', 'webhook', 'fleet_error', '{}'::jsonb, 2000, 'provider_error', 'upstream refused', 51, 51),
      ('01950000-0000-7000-8000-000000000012', '01950000-0000-7000-8000-000000000003', 'event-c', 'user:c', 'chat', 'received', '{}'::jsonb, NULL, NULL, NULL, 52, 52)
)
INSERT INTO fleet.runner_leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor, event_type,
   event_created_at, posture, provider, model, metered_input_tokens,
   metered_cached_tokens, metered_output_tokens, last_metered_at, fencing_token,
   lease_expires_at, status, created_at, updated_at)
VALUES
  ('01950000-0000-7000-8000-000000000031', '01950000-0000-7000-8000-000000000021', '01950000-0000-7000-8000-000000000011', '01950000-0000-7000-8000-000000000002', '01950000-0000-7000-8000-000000000001', 'event-a', 'user:a', 'chat', 50, 'platform', 'anthropic', 'model-a', 10, 2, 3, 100, 1, 130, 'expired', 100, 100),
  ('01950000-0000-7000-8000-000000000032', '01950000-0000-7000-8000-000000000021', '01950000-0000-7000-8000-000000000011', '01950000-0000-7000-8000-000000000002', '01950000-0000-7000-8000-000000000001', 'event-a', 'user:a', 'chat', 50, 'platform', 'anthropic', 'model-a', 20, 4, 6, 200, 2, 230, 'reported', 200, 200),
  ('01950000-0000-7000-8000-000000000033', '01950000-0000-7000-8000-000000000021', '01950000-0000-7000-8000-000000000012', '01950000-0000-7000-8000-000000000003', '01950000-0000-7000-8000-000000000001', 'event-c', 'user:c', 'chat', 52, 'bring_your_own', 'openai', 'model-c', 30, 6, 9, 300, 1, 330, 'active', 300, 300),
  ('01950000-0000-7000-8000-000000000034', '01950000-0000-7000-8000-000000000021', '01950000-0000-7000-8000-000000000012', '01950000-0000-7000-8000-000000000003', '01950000-0000-7000-8000-000000000001', 'event-b', 'user:b', 'webhook', 51, 'platform', 'anthropic', 'model-b', 40, 8, 12, 300, 1, 330, 'reported', 300, 300),
  ('01950000-0000-7000-8000-000000000041', '01950000-0000-7000-8000-000000000022', '01950000-0000-7000-8000-000000000011', '01950000-0000-7000-8000-000000000002', '01950000-0000-7000-8000-000000000001', 'event-a', 'user:a', 'chat', 50, 'platform', 'anthropic', 'model-a', 1, 0, 1, 400, 3, 430, 'reported', 400, 400)
";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn runner_lease_history_pages_filters_and_scopes_cursors() {
    let fixtures = Fixtures::create().await;
    fixtures.seed().await;
    let history = RunnerLeaseHistory::new(fixtures.database.clone());
    let runner = id(RUNNER_A);

    let first = history
        .list(&runner, None, None, None, 2)
        .await
        .expect("the first page reads");
    assert_eq!(first.total, 4);
    assert_eq!(ids(&first.items), [LEASE_4, LEASE_3]);
    let failed = first.items.first().expect("the failed row is present");
    let running = first.items.get(1).expect("the running row is present");
    assert_eq!(failed.outcome, LeaseOutcome::Failed);
    assert_eq!(failed.failure_label.as_deref(), Some("provider_error"));
    assert_eq!(running.outcome, LeaseOutcome::Running);
    assert_eq!(first.next_cursor.as_deref(), Some(LEASE_3));

    let second = history
        .list(&runner, None, None, Some(&id(LEASE_3)), 2)
        .await
        .expect("the next page reads");
    assert_eq!(second.total, 4);
    assert_eq!(ids(&second.items), [LEASE_2, LEASE_1]);
    let succeeded = second.items.first().expect("the succeeded row is present");
    let expired = second.items.get(1).expect("the expired row is present");
    assert_eq!(succeeded.outcome, LeaseOutcome::Succeeded);
    assert_eq!(succeeded.kind, LeaseKind::Reclaim);
    assert_eq!(expired.outcome, LeaseOutcome::Expired);

    let production = history
        .list(&runner, None, Some("production"), None, 100)
        .await
        .expect("fleet names match case-insensitively");
    assert_eq!(production.total, 2);
    assert_eq!(ids(&production.items), [LEASE_2, LEASE_1]);

    let secondary = history
        .list(&runner, Some(&id(WORKSPACE_B)), None, None, 100)
        .await
        .expect("workspace filtering reads");
    assert_eq!(secondary.total, 2);
    assert_eq!(ids(&secondary.items), [LEASE_4, LEASE_3]);

    for cursor in [FOREIGN_LEASE, LEASE_2] {
        let workspace = (cursor == LEASE_2).then(|| id(WORKSPACE_B));
        let refused = history
            .list(&runner, workspace.as_ref(), None, Some(&id(cursor)), 50)
            .await
            .expect_err("a cursor outside this filtered runner stream is refused");
        assert_eq!(refused.code().as_str(), "UZ-REQ-001");
    }

    let missing = history
        .list(
            &id("01950000-0000-7000-8000-000000000099"),
            None,
            None,
            None,
            50,
        )
        .await
        .expect_err("an unknown runner is distinct from an empty history");
    assert_eq!(missing.code(), error_code::RUNNER_NOT_FOUND);

    fixtures.cleanup().await;
}

fn id(raw: &str) -> Uuid7 {
    Uuid7::parse(raw).expect("the fixture id is UUIDv7")
}

fn ids<'a>(items: &'a [afd_wire::operator::RunnerLeaseItem<'_>]) -> Vec<&'a str> {
    items.iter().map(|item| item.id.as_ref()).collect()
}

struct Fixtures {
    base_url: String,
    name: String,
    database: Db,
}

impl Fixtures {
    async fn create() -> Self {
        let base_url = std::env::var(LANE_KNOB).unwrap_or_else(|_error| {
            panic!("{LANE_KNOB} is unset — run through `make test-integration-rustd`")
        });
        let name = format!(
            "afd_fleet_ops_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        admin(&base_url, AssertSqlSafe(format!("CREATE DATABASE {name}"))).await;
        let url = database_url(&base_url, &name);
        let migrator = open(&url, DbRole::Migrator).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the schema applies");
        drop(migrator);
        Self {
            database: open(&url, DbRole::Api).await,
            base_url,
            name,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(SEED)
            .execute(&mut *connection)
            .await
            .expect("the operator projection fixture seeds");
    }

    async fn cleanup(self) {
        drop(self.database);
        admin(
            &self.base_url,
            AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            )),
        )
        .await;
    }
}

fn database_url(base_url: &str, name: &str) -> String {
    let (prefix, tail) = base_url
        .rsplit_once('/')
        .expect("a Postgres URL has a database path");
    let query = tail.split_once('?').map_or("", |(_, query)| query);
    if query.is_empty() {
        format!("{prefix}/{name}")
    } else {
        format!("{prefix}/{name}?{query}")
    }
}

async fn open(url: &str, role: DbRole) -> Db {
    let env = MapEnv::from_pairs(DbRole::ALL.iter().map(|each| (each.url_knob(), url)));
    Db::connect(&PoolConfig::resolve(&env, role).expect("the URL resolves"))
        .await
        .expect("the database accepts a connection")
}

async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
    let pool = sqlx::PgPool::connect(base_url)
        .await
        .expect("the lane database is reachable");
    let mut connection = pool.acquire().await.expect("an admin connection");
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .expect("the admin statement runs");
    drop(connection);
    pool.close().await;
}
