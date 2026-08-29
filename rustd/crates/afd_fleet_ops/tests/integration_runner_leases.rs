//! Runner lease-history proofs against the migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_wire::operator::{LeaseKind, LeaseOutcome};

const SEED: &str = r"
WITH inserted_tenant AS (
    INSERT INTO core.tenants (id, name, created_at, updated_at)
    VALUES ($1::uuid, 'Operator test', 1, 1)
), inserted_workspaces AS (
    INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
    VALUES
      ($2::uuid, $1::uuid, 'primary', 'test', 2),
      ($3::uuid, $1::uuid, 'secondary', 'test', 3)
), inserted_fleets AS (
    INSERT INTO core.fleets
      (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
    VALUES
      ($4::uuid, $2::uuid, $1::uuid, 'Production', '# prod', '{}'::jsonb, 'active', 10, 10),
      ($5::uuid, $3::uuid, $1::uuid, 'Canary', '# canary', '{}'::jsonb, 'active', 11, 11)
), inserted_runners AS (
    INSERT INTO fleet.runners
      (id, host_id, token_hash, sandbox_tier, admin_state, labels, last_seen_at, created_at, updated_at)
    VALUES
      ($6::uuid, $16, $18, 'dev_none', 'active', '[]'::jsonb, 20, 20, 20),
      ($7::uuid, $17, $19, 'dev_none', 'active', '[]'::jsonb, 21, 21, 21)
), inserted_events AS (
    INSERT INTO core.fleet_events
      (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, wall_ms, failure_label, failure_detail, created_at, updated_at)
    VALUES
      ($4::uuid, $2::uuid, $13, 'user:a', 'chat', 'processed', '{}'::jsonb, 999, NULL, NULL, 50, 50),
      ($5::uuid, $3::uuid, $14, 'user:b', 'webhook', 'fleet_error', '{}'::jsonb, 2000, 'provider_error', 'upstream refused', 51, 51),
      ($5::uuid, $3::uuid, $15, 'user:c', 'chat', 'received', '{}'::jsonb, NULL, NULL, NULL, 52, 52)
)
INSERT INTO fleet.runner_leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor, event_type,
   event_created_at, posture, provider, model, metered_input_tokens,
   metered_cached_tokens, metered_output_tokens, last_metered_at, fencing_token,
   lease_expires_at, status, created_at, updated_at)
VALUES
  ($8::uuid, $6::uuid, $4::uuid, $2::uuid, $1::uuid, $13, 'user:a', 'chat', 50, 'platform', 'anthropic', 'model-a', 10, 2, 3, 100, 1, 130, 'expired', 100, 100),
  ($9::uuid, $6::uuid, $4::uuid, $2::uuid, $1::uuid, $13, 'user:a', 'chat', 50, 'platform', 'anthropic', 'model-a', 20, 4, 6, 200, 2, 230, 'reported', 200, 200),
  ($10::uuid, $6::uuid, $5::uuid, $3::uuid, $1::uuid, $15, 'user:c', 'chat', 52, 'bring_your_own', 'openai', 'model-c', 30, 6, 9, 300, 1, 330, 'active', 300, 300),
  ($11::uuid, $6::uuid, $5::uuid, $3::uuid, $1::uuid, $14, 'user:b', 'webhook', 51, 'platform', 'anthropic', 'model-b', 40, 8, 12, 300, 1, 330, 'reported', 300, 300),
  ($12::uuid, $7::uuid, $4::uuid, $2::uuid, $1::uuid, $13, 'user:a', 'chat', 50, 'platform', 'anthropic', 'model-a', 1, 0, 1, 400, 3, 430, 'reported', 400, 400)
";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn runner_lease_history_pages_filters_and_scopes_cursors() {
    let fixtures = Fixtures::create().await;
    fixtures.seed().await;
    let history = RunnerLeaseHistory::new(fixtures.database.clone());
    let runner = id(&fixtures.runner_a);

    assert_history_pages(&history, &fixtures, &runner).await;
    assert_history_filters(&history, &fixtures, &runner).await;
    assert_history_refusals(&history, &fixtures, &runner).await;
    fixtures.cleanup().await;
}

async fn assert_history_pages(history: &RunnerLeaseHistory, fixtures: &Fixtures, runner: &Uuid7) {
    let first = history
        .list(runner, None, None, None, 2)
        .await
        .expect("the first page reads");
    assert_eq!(first.total, 4);
    assert_eq!(
        ids(&first.items),
        [fixtures.lease_4.as_str(), fixtures.lease_3.as_str()]
    );
    let failed = first.items.first().expect("the failed row is present");
    let running = first.items.get(1).expect("the running row is present");
    assert_eq!(failed.outcome, LeaseOutcome::Failed);
    assert_eq!(failed.failure_label.as_deref(), Some("provider_error"));
    assert_eq!(running.outcome, LeaseOutcome::Running);
    assert_eq!(
        first.next_cursor.as_deref(),
        Some(fixtures.lease_3.as_str())
    );

    let second = history
        .list(runner, None, None, Some(&id(&fixtures.lease_3)), 2)
        .await
        .expect("the next page reads");
    assert_eq!(second.total, 4);
    assert_eq!(
        ids(&second.items),
        [fixtures.lease_2.as_str(), fixtures.lease_1.as_str()]
    );
    let succeeded = second.items.first().expect("the succeeded row is present");
    let expired = second.items.get(1).expect("the expired row is present");
    assert_eq!(succeeded.outcome, LeaseOutcome::Succeeded);
    assert_eq!(succeeded.kind, LeaseKind::Reclaim);
    assert_eq!(expired.outcome, LeaseOutcome::Expired);
}

async fn assert_history_filters(history: &RunnerLeaseHistory, fixtures: &Fixtures, runner: &Uuid7) {
    let production = history
        .list(runner, None, Some("production"), None, 100)
        .await
        .expect("fleet names match case-insensitively");
    assert_eq!(production.total, 2);
    assert_eq!(
        ids(&production.items),
        [fixtures.lease_2.as_str(), fixtures.lease_1.as_str()]
    );

    let secondary = history
        .list(runner, Some(&id(&fixtures.workspace_b)), None, None, 100)
        .await
        .expect("workspace filtering reads");
    assert_eq!(secondary.total, 2);
    assert_eq!(
        ids(&secondary.items),
        [fixtures.lease_4.as_str(), fixtures.lease_3.as_str()]
    );
}

async fn assert_history_refusals(
    history: &RunnerLeaseHistory,
    fixtures: &Fixtures,
    runner: &Uuid7,
) {
    for cursor in [&fixtures.foreign_lease, &fixtures.lease_2] {
        let workspace = (cursor == &fixtures.lease_2).then(|| id(&fixtures.workspace_b));
        let refused = history
            .list(runner, workspace.as_ref(), None, Some(&id(cursor)), 50)
            .await
            .expect_err("a cursor outside this filtered runner stream is refused");
        assert_eq!(refused.code().as_str(), "UZ-REQ-001");
    }

    let missing = history
        .list(&id(&fixtures.missing_runner), None, None, None, 50)
        .await
        .expect_err("an unknown runner is distinct from an empty history");
    assert_eq!(missing.code(), error_code::RUNNER_NOT_FOUND);
}

fn id(raw: &str) -> Uuid7 {
    Uuid7::parse(raw).expect("the fixture id is UUIDv7")
}

fn ids<'a>(items: &'a [afd_wire::operator::RunnerLeaseItem<'_>]) -> Vec<&'a str> {
    items.iter().map(|item| item.id.as_ref()).collect()
}

struct Fixtures {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace_a: String,
    workspace_b: String,
    fleet_a: String,
    fleet_b: String,
    runner_a: String,
    runner_b: String,
    lease_1: String,
    lease_2: String,
    lease_3: String,
    lease_4: String,
    foreign_lease: String,
    missing_runner: String,
    event_a: String,
    event_b: String,
    event_c: String,
    host_a: String,
    host_b: String,
    hash_a: String,
    hash_b: String,
}

impl Fixtures {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let suffix = afd_db::test_util::mint_id();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: afd_db::test_util::mint_id(),
            workspace_a: afd_db::test_util::mint_id(),
            workspace_b: afd_db::test_util::mint_id(),
            fleet_a: afd_db::test_util::mint_id(),
            fleet_b: afd_db::test_util::mint_id(),
            runner_a: afd_db::test_util::mint_id(),
            runner_b: afd_db::test_util::mint_id(),
            lease_1: afd_db::test_util::mint_id(),
            lease_2: afd_db::test_util::mint_id(),
            lease_3: afd_db::test_util::mint_id(),
            lease_4: afd_db::test_util::mint_id(),
            foreign_lease: afd_db::test_util::mint_id(),
            missing_runner: afd_db::test_util::mint_id(),
            event_a: format!("event-a-{suffix}"),
            event_b: format!("event-b-{suffix}"),
            event_c: format!("event-c-{suffix}"),
            host_a: format!("host-a-{suffix}"),
            host_b: format!("host-b-{suffix}"),
            hash_a: format!("hash-a-{suffix}"),
            hash_b: format!("hash-b-{suffix}"),
            lane,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(SEED)
            .bind(&self.tenant)
            .bind(&self.workspace_a)
            .bind(&self.workspace_b)
            .bind(&self.fleet_a)
            .bind(&self.fleet_b)
            .bind(&self.runner_a)
            .bind(&self.runner_b)
            .bind(&self.lease_1)
            .bind(&self.lease_2)
            .bind(&self.lease_3)
            .bind(&self.lease_4)
            .bind(&self.foreign_lease)
            .bind(&self.event_a)
            .bind(&self.event_b)
            .bind(&self.event_c)
            .bind(&self.host_a)
            .bind(&self.host_b)
            .bind(&self.hash_a)
            .bind(&self.hash_b)
            .execute(&mut *connection)
            .await
            .expect("the operator projection fixture seeds");
    }

    async fn cleanup(self) {
        drop(self.database);
        self.lane.cleanup().await;
    }
}
