//! Each proof owns a runner and workspace; counts never inspect other tests.

use afd_core::{clock, id::Uuid7};
use afd_db::{
    Db,
    config::DbRole,
    test_util::{FleetSeed, TestDatabase, mint_id},
};
use sqlx::{Connection as _, Postgres, Transaction};

pub(super) struct Fixture {
    pub(super) db: Db,
    pub(super) runner: Uuid7,
    tenant: String,
    workspace: String,
    fleet: String,
    lane: TestDatabase,
}

const SEED: &str = "\
WITH tenant AS (
  INSERT INTO core.tenants(id, name, created_at, updated_at)
  VALUES ($1::uuid, 'capacity', $4, $4) RETURNING id
), workspace AS (
  INSERT INTO core.workspaces(id, tenant_id, created_at)
  SELECT $2::uuid, id, $4 FROM tenant
)
INSERT INTO fleet.runners(id, host_id, token_hash, sandbox_tier, admin_state,
  labels, network_policy, registry_allowlist, worker_count, degraded, last_seen_at,
  created_at, updated_at)
VALUES ($3::uuid, $3, $3, 'dev_none', $6, '[]', 'deny_all_egress', '[]', $5, false,
  $4, $4, $4)";

const INSERT_AUTHORING: &str = "\
INSERT INTO fleet.authoring_attempts(id, lease_id, runner_id, workspace_id,
  tenant_id, creator_id, draft_id, message_sequence, fence, deletion_generation,
  capability_hash, model_snapshot, status, lease_expires_at, deadline,
  reserved_nanos, settled_nanos, usage_unknown, calls, input_tokens, output_tokens,
  activity_sequence, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6::uuid, $6::uuid,
  $7, 1, 1, $1, '{}', $8, $9, $9, 0, 0, false, 0, 0, 0, 0, $10, $10)";

const INSERT_RUNTIME: &str = "\
INSERT INTO fleet.runner_leases(id, runner_id, fleet_id, workspace_id, tenant_id,
 event_id, receipt, actor, event_type, event_created_at, posture, provider, model,
 metered_input_tokens, metered_cached_tokens, metered_output_tokens,
 last_metered_at, fencing_token, lease_expires_at, status, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $1, '1-0', 'capacity', 'chat',
 $6, 'tenant', 'test', 'test', 0, 0, 0, $6, 1, $7, $8, $6, $6)";

impl Fixture {
    pub(super) async fn create(workers: i32) -> Self {
        let lane = TestDatabase::shared();
        let fixture = Self {
            db: lane.open(DbRole::Api, &[]).await,
            runner: Uuid7::parse(&mint_id()).expect("fixture runner UUIDv7"),
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
            lane,
        };
        let now = clock::now().as_millis();
        let mut connection = fixture.db.acquire().await.expect("seed connection");
        sqlx::query(SEED)
            .bind(&fixture.tenant)
            .bind(&fixture.workspace)
            .bind(fixture.runner.as_str())
            .bind(now)
            .bind(workers)
            .bind(afd_runner::sql::ADMIN_STATE_ACTIVE)
            .execute(&mut *connection)
            .await
            .expect("seed isolated capacity host");
        FleetSeed::new(
            &fixture.fleet,
            &fixture.workspace,
            &fixture.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("capacity")
        .at(now)
        .insert(&mut *connection)
        .await
        .expect("seed isolated capacity host");
        drop(connection);
        fixture
    }

    pub(super) async fn authoring(&self, tx: &mut Transaction<'_, Postgres>, until: i64) {
        sqlx::query(INSERT_AUTHORING)
            .bind(mint_id())
            .bind(mint_id())
            .bind(self.runner.as_str())
            .bind(&self.workspace)
            .bind(&self.tenant)
            .bind(mint_id())
            .bind(1_i64)
            .bind(afd_runner::sql::LEASE_STATUS_ACTIVE)
            .bind(until)
            .bind(clock::now().as_millis())
            .execute(&mut **tx)
            .await
            .expect("authoring slot is recorded under the host lock");
    }

    pub(super) async fn runtime(&self, until: i64) {
        let mut connection = self.db.acquire().await.expect("runtime seed connection");
        sqlx::query(INSERT_RUNTIME)
            .bind(mint_id())
            .bind(self.runner.as_str())
            .bind(&self.fleet)
            .bind(&self.workspace)
            .bind(&self.tenant)
            .bind(clock::now().as_millis())
            .bind(until)
            .bind(afd_runner::sql::LEASE_STATUS_ACTIVE)
            .execute(&mut *connection)
            .await
            .expect("runtime slot seeds");
    }

    pub(super) async fn try_claim(&self) -> bool {
        let mut connection = self.db.acquire().await.expect("claim connection");
        let mut tx = connection.begin().await.expect("claim transaction");
        let capacity = afd_runner::capacity::lock(&mut tx, &self.runner)
            .await
            .expect("lock host")
            .expect("isolated host exists");
        let accepted = capacity.can_claim();
        if accepted {
            self.authoring(&mut tx, capacity.now().as_millis() + 60_000)
                .await;
        }
        tx.commit().await.expect("commit the guarded claim");
        accepted
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.db.acquire().await.expect("cleanup connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("remove only this test's rows");
        sqlx::query("DELETE FROM fleet.runners WHERE id = $1::uuid")
            .bind(self.runner.as_str())
            .execute(&mut *connection)
            .await
            .expect("remove only this test's host");
        drop(connection);
        self.db.close().await;
        self.lane.cleanup().await;
    }
}
