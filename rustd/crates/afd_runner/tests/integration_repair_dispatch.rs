//! Repair-verification dispatch through live Postgres and Redis.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::repair::Repairs;
use sqlx::Acquire as _;

use crate::support::connect_redis;

const NOW: i64 = 1_760_000_000_000;

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn repair_dispatch_records_one_event_and_cleans_its_retry_key() {
    let fixture = Fixture::create().await;
    fixture.seed_intent().await;
    let queue = connect_redis().await;
    let repairs = Repairs::new(fixture.database.clone(), queue, Entropy::new());

    let first = repairs.sweep().await.expect("the due intent dispatches");
    assert!(first.scanned >= 1, "the fixture intent is scanned");
    assert!(first.changed >= 1, "the fixture intent dispatches");
    let recorded = fixture.verification().await;
    assert!(recorded.event_id.is_some());
    assert_eq!(recorded.attempts, 1);
    assert!(recorded.once_key_cleared_at.is_some());

    repairs
        .sweep()
        .await
        .expect("the completed intent stays done");
    assert_eq!(fixture.verification().await, recorded);

    fixture.cleanup().await;
}

#[derive(Debug, PartialEq, Eq)]
struct Verification {
    event_id: Option<String>,
    attempts: i64,
    once_key_cleared_at: Option<i64>,
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: String,
    incident_fleet: String,
    verifier_fleet: String,
    incident_event: String,
    repair_link: String,
    production: String,
    verification: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: mint_id(),
            incident_fleet: mint_id(),
            verifier_fleet: mint_id(),
            incident_event: format!("evt_{}", mint_id()),
            repair_link: mint_id(),
            production: mint_id(),
            verification: mint_id(),
            lane,
        }
    }

    async fn seed_intent(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        self.seed_scope(&mut connection).await;
        self.seed_incident(&mut connection).await;
        self.seed_repair(&mut connection).await;
    }

    async fn seed_scope(&self, connection: &mut sqlx::PgConnection) {
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Repair dispatch', $5, $5) \
               RETURNING id \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               SELECT $2::uuid, id, $2, 'test', $5 FROM tenant \
               RETURNING id, tenant_id \
             ) \
             INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at) \
             SELECT $3::uuid, id, tenant_id, $3, '# incident', '{}'::jsonb, 'active', $5, $5 \
             FROM workspace \
             UNION ALL \
             SELECT $4::uuid, id, tenant_id, $4, '# verifier', '{}'::jsonb, 'active', $5, $5 \
             FROM workspace",
        )
        .bind(&self.tenant)
        .bind(&self.workspace)
        .bind(&self.incident_fleet)
        .bind(&self.verifier_fleet)
        .bind(NOW)
        .execute(connection)
        .await
        .expect("the repair scope seeds");
    }

    async fn seed_incident(&self, connection: &mut sqlx::PgConnection) {
        sqlx::query(
            "INSERT INTO core.fleet_events \
               (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, \
                response_text, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, 'test', 'chat', 'fleet_error', \
                     '{\"question\":\"why\"}', 'because', $4, $4)",
        )
        .bind(&self.incident_fleet)
        .bind(&self.workspace)
        .bind(&self.incident_event)
        .bind(NOW)
        .execute(connection)
        .await
        .expect("the incident event seeds");
    }

    async fn seed_repair(&self, connection: &mut sqlx::PgConnection) {
        sqlx::query(
            "WITH repair_link AS ( \
               INSERT INTO core.repair_pr_links \
               (id, workspace_id, fleet_id, event_id, repository, branch, pr_number, pr_url, \
                deploy_status, merged_commit_sha, merged_at, created_at) \
               VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'agentsfleet/test', 'repair/test', 7, \
                       'https://example.test/pr/7', 'deploy_ok', 'abc123', $8, $8) \
               RETURNING id \
             ), production AS ( \
               INSERT INTO core.repair_production_results \
               (id, workspace_id, provider, provider_deployment_id, provider_status_id, repository, \
                environment, commit_sha, conclusion, completed_at, created_at) \
               VALUES ($5::uuid, $2::uuid, 'github', $5, $5, 'agentsfleet/test', 'production', \
                       'abc123', 'success', $8, $8) \
               RETURNING id \
             ) \
             INSERT INTO core.repair_verifications \
               (id, workspace_id, production_result_id, repair_link_id, verifier_fleet_id, \
                verify_after, dispatch_attempts, created_at, updated_at) \
             SELECT $6::uuid, $2::uuid, production.id, repair_link.id, $7::uuid, 1, 0, $8, $8 \
             FROM production CROSS JOIN repair_link",
        )
        .bind(&self.repair_link)
        .bind(&self.workspace)
        .bind(&self.incident_fleet)
        .bind(&self.incident_event)
        .bind(&self.production)
        .bind(&self.verification)
        .bind(&self.verifier_fleet)
        .bind(NOW)
        .execute(connection)
        .await
        .expect("the repair verification seeds");
    }

    async fn verification(&self) -> Verification {
        use sqlx::Row as _;

        let mut connection = self.database.acquire().await.expect("an API connection");
        let row = sqlx::query(
            "SELECT verifier_event_id, dispatch_attempts, redis_once_key_cleared_at \
             FROM core.repair_verifications WHERE id = $1::uuid",
        )
        .bind(&self.verification)
        .fetch_one(&mut *connection)
        .await
        .expect("the scoped verification reads");
        Verification {
            event_id: row.try_get(0).expect("the event id shape is readable"),
            attempts: row.try_get(1).expect("the attempts shape is readable"),
            once_key_cleared_at: row.try_get(2).expect("the cleanup shape is readable"),
        }
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = connection.begin().await.expect("cleanup begins");
        sqlx::query("SET LOCAL fleet.allow_gate_purge = 'on'")
            .execute(&mut *transaction)
            .await
            .expect("the sanctioned history purge is enabled");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the scoped fixture cleans up");
        transaction.commit().await.expect("cleanup commits");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
