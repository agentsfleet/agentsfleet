//! Runner lease-history proofs against the migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_api_wire::operator::{LeaseDestination, LeaseKind, LeaseOutcome};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_uuid7};
use afd_fleet_ops::RunnerLeaseHistory;
use afd_state::sql::FLEET_STATUS_ACTIVE;
use jiff::Timestamp;

/// When the production fleet and then the canary fleet were installed.
const PRODUCTION_INSTALLED: Timestamp = Timestamp::constant(1_760_000_000, 10_000_000);
const CANARY_INSTALLED: Timestamp = Timestamp::constant(1_760_000_000, 11_000_000);

/// Two runners, three events on them, and five leases: `$6` holds four, `$7`
/// one. Events `$13` and `$15` are processed and received; `$14` failed.
/// `$20`–`$22` are the event rows' ids.
const SEED: &str = r"
WITH inserted_runners AS (
    INSERT INTO runner.runners
      (id, host_id, token_hash, sandbox_tier, status, labels, network_policy,
       registry_allowlist, worker_count, lease_version, last_seen_at, created_at, updated_at)
    VALUES
      ($6::uuid, $16, $18, 'dev_none', 'active', '[]'::jsonb, 'deny_all_egress', '[]'::jsonb,
       1, 0, '2025-10-09 08:00:00.020+00', '2025-10-09 08:00:00.020+00', '2025-10-09 08:00:00.020+00'),
      ($7::uuid, $17, $19, 'dev_none', 'active', '[]'::jsonb, 'deny_all_egress', '[]'::jsonb,
       1, 0, '2025-10-09 08:00:00.021+00', '2025-10-09 08:00:00.021+00', '2025-10-09 08:00:00.021+00')
), inserted_events AS (
    INSERT INTO fleet.events
      (id, fleet_id, workspace_id, event_id, actor, event_type, status, request_json, wall_ms,
       failure_reason, failure_detail, created_at, updated_at)
    VALUES
      ($20::uuid, $4::uuid, $2::uuid, $13, 'user:a', 'chat', 'processed', '{}'::jsonb, 999,
       NULL, NULL, '2025-10-09 08:00:00.050+00', '2025-10-09 08:00:00.050+00'),
      ($21::uuid, $5::uuid, $3::uuid, $14, 'user:b', 'webhook', 'fleet_error', '{}'::jsonb, 2000,
       'provider_error', 'upstream refused', '2025-10-09 08:00:00.051+00', '2025-10-09 08:00:00.051+00'),
      ($22::uuid, $5::uuid, $3::uuid, $15, 'user:c', 'chat', 'received', '{}'::jsonb, NULL,
       NULL, NULL, '2025-10-09 08:00:00.052+00', '2025-10-09 08:00:00.052+00')

)
INSERT INTO runner.leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, fleet_event_id, event_created_at,
   receipt, posture, provider, model, last_metered_at, fencing_token, lease_expires_at, status, posted_message_count,
   created_at, updated_at)
SELECT v.id, v.runner_id, v.fleet_id, v.workspace_id, $1::uuid, v.fleet_event_id,
       v.event_created_at, v.receipt, v.posture, v.provider, v.model, v.created_at,
       v.fencing_token, v.expires_at, v.status, 0, v.created_at, v.created_at
FROM (VALUES
  ($8::uuid, $6::uuid, $4::uuid, $2::uuid, $20::uuid, '2025-10-09 08:00:00.050+00'::timestamptz, $13, 'platform', 'anthropic', 'model-a', 10, 2, 3, 1, '2025-10-09 08:00:00.130+00'::timestamptz, 'expired', '2025-10-09 08:00:00.100+00'::timestamptz),
  ($9::uuid, $6::uuid, $4::uuid, $2::uuid, $20::uuid, '2025-10-09 08:00:00.050+00', $13, 'platform', 'anthropic', 'model-a', 20, 4, 6, 2, '2025-10-09 08:00:00.230+00', 'reported', '2025-10-09 08:00:00.200+00'),
  ($10::uuid, $6::uuid, $5::uuid, $3::uuid, $22::uuid, '2025-10-09 08:00:00.052+00', $15, 'bring_your_own', 'openai', 'model-c', 30, 6, 9, 1, '2025-10-09 08:00:00.330+00', 'active', '2025-10-09 08:00:00.300+00'),
  ($11::uuid, $6::uuid, $5::uuid, $3::uuid, $21::uuid, '2025-10-09 08:00:00.051+00', $14, 'platform', 'anthropic', 'model-b', 40, 8, 12, 1, '2025-10-09 08:00:00.330+00', 'reported', '2025-10-09 08:00:00.300+00'),
  ($12::uuid, $7::uuid, $4::uuid, $2::uuid, $20::uuid, '2025-10-09 08:00:00.050+00', $13, 'platform', 'anthropic', 'model-a', 1, 0, 1, 3, '2025-10-09 08:00:00.430+00', 'reported', '2025-10-09 08:00:00.400+00')
) AS v(id, runner_id, fleet_id, workspace_id, fleet_event_id, event_created_at, receipt,
       posture, provider, model, input, cached, output, fencing_token, expires_at, status,
       created_at)
";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn runner_lease_history_pages_filters_and_scopes_cursors() {
    let fixtures = Fixtures::create().await;
    fixtures.seed().await;
    let history = RunnerLeaseHistory::new(fixtures.database.clone());
    let runner = &fixtures.runner_a;

    assert_history_pages(&history, &fixtures, runner).await;
    assert_history_filters(&history, &fixtures, runner).await;
    assert_history_refusals(&history, &fixtures, runner).await;
    fixtures.cleanup().await;
}

async fn assert_history_pages(history: &RunnerLeaseHistory, fixtures: &Fixtures, runner: &Uuid7) {
    let first = history
        .list(&fixtures.tenant, runner, None, None, None, 2)
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
    assert_eq!(
        failed.destination,
        LeaseDestination::Available,
        "a live fleet in the viewer's own account is a destination"
    );
    assert_eq!(failed.failure_label.as_deref(), Some("provider_error"));
    assert_eq!(running.outcome, LeaseOutcome::Running);
    assert_eq!(
        first.next_cursor.as_deref(),
        Some(fixtures.lease_3.as_str())
    );

    let second = history
        .list(
            &fixtures.tenant,
            runner,
            None,
            None,
            Some(&fixtures.lease_3),
            2,
        )
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
        .list(
            &fixtures.tenant,
            runner,
            None,
            Some("production"),
            None,
            100,
        )
        .await
        .expect("fleet names match case-insensitively");
    assert_eq!(production.total, 2);
    assert_eq!(
        ids(&production.items),
        [fixtures.lease_2.as_str(), fixtures.lease_1.as_str()]
    );

    let secondary = history
        .list(
            &fixtures.tenant,
            runner,
            Some(&fixtures.workspace_b),
            None,
            None,
            100,
        )
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
        let workspace = (cursor == &fixtures.lease_2).then_some(&fixtures.workspace_b);
        let refused = history
            .list(&fixtures.tenant, runner, workspace, None, Some(cursor), 50)
            .await
            .expect_err("a cursor outside this filtered runner stream is refused");
        assert_eq!(refused.code().as_str(), "UZ-REQ-001");
    }

    let missing = history
        .list(
            &fixtures.tenant,
            &fixtures.missing_runner,
            None,
            None,
            None,
            50,
        )
        .await
        .expect_err("an unknown runner is distinct from an empty history");
    assert_eq!(missing.code(), error_code::RUNNER_NOT_FOUND);
}

fn ids<'a>(items: &'a [afd_api_wire::operator::RunnerLeaseItem<'_>]) -> Vec<&'a str> {
    items.iter().map(|item| item.id.as_ref()).collect()
}

struct Fixtures {
    lane: TestDatabase,
    database: Db,
    tenant: Uuid7,
    workspace_a: Uuid7,
    workspace_b: Uuid7,
    fleet_a: Uuid7,
    fleet_b: Uuid7,
    runner_a: Uuid7,
    runner_b: Uuid7,
    lease_1: Uuid7,
    lease_2: Uuid7,
    lease_3: Uuid7,
    lease_4: Uuid7,
    foreign_lease: Uuid7,
    missing_runner: Uuid7,
    events: [Uuid7; 3],
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
        let suffix = mint_uuid7();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_uuid7(),
            workspace_a: mint_uuid7(),
            workspace_b: mint_uuid7(),
            fleet_a: mint_uuid7(),
            fleet_b: mint_uuid7(),
            runner_a: mint_uuid7(),
            runner_b: mint_uuid7(),
            lease_1: mint_uuid7(),
            lease_2: mint_uuid7(),
            lease_3: mint_uuid7(),
            lease_4: mint_uuid7(),
            foreign_lease: mint_uuid7(),
            missing_runner: mint_uuid7(),
            events: [mint_uuid7(), mint_uuid7(), mint_uuid7()],
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
        TenantSeed::new(self.tenant.as_str())
            .name("Operator test")
            .insert(&mut *connection)
            .await
            .expect("the operator projection fixture seeds");
        for (workspace, name) in [
            (&self.workspace_a, "primary"),
            (&self.workspace_b, "secondary"),
        ] {
            WorkspaceSeed::new(workspace.as_str(), self.tenant.as_str())
                .name(name)
                .insert(&mut *connection)
                .await
                .expect("the operator projection fixture seeds");
        }
        for (fleet, workspace, name, at) in [
            (
                &self.fleet_a,
                &self.workspace_a,
                "Production",
                PRODUCTION_INSTALLED,
            ),
            (&self.fleet_b, &self.workspace_b, "Canary", CANARY_INSTALLED),
        ] {
            FleetSeed::new(
                fleet.as_str(),
                workspace.as_str(),
                self.tenant.as_str(),
                FLEET_STATUS_ACTIVE,
            )
            .name(name)
            .at(at)
            .insert(&mut *connection)
            .await
            .expect("the operator projection fixture seeds");
        }
        self.seed_leases(&mut connection).await;
    }

    /// The runners, events and leases [`SEED`] writes, bound in its `$n` order.
    async fn seed_leases(&self, connection: &mut sqlx::PgConnection) {
        let ids = [
            &self.tenant,
            &self.workspace_a,
            &self.workspace_b,
            &self.fleet_a,
            &self.fleet_b,
            &self.runner_a,
            &self.runner_b,
            &self.lease_1,
            &self.lease_2,
            &self.lease_3,
            &self.lease_4,
            &self.foreign_lease,
        ];
        let texts = [
            &self.event_a,
            &self.event_b,
            &self.event_c,
            &self.host_a,
            &self.host_b,
            &self.hash_a,
            &self.hash_b,
        ];
        let seeded = ids
            .into_iter()
            .map(Uuid7::as_str)
            .chain(texts.into_iter().map(String::as_str))
            .chain(self.events.iter().map(Uuid7::as_str))
            .fold(sqlx::query(SEED), sqlx::query::Query::bind);
        seeded
            .execute(connection)
            .await
            .expect("the operator projection fixture seeds");
    }

    async fn cleanup(self) {
        drop(self.database);
        self.lane.cleanup().await;
    }
}
