//! Runner sweep transitions against the lane's migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::liveness::Liveness;
use afd_runner::sweep::retention::Retention;

const OLD: i64 = 1;
const NEVER_SEEN: i64 = 0;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn liveness_and_retention_converge_real_runner_rows() {
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let liveness = Liveness::new(fixture.database.clone(), Entropy::new());
    let first = liveness.sweep().await.expect("liveness sweeps real rows");
    assert!(first.scanned >= 4, "the four fixture runners are scanned");
    assert!(first.changed >= 4, "the four stale fixture rows converge");
    assert_eq!(fixture.runner_state(&fixture.draining).await, "drained");
    assert!(fixture.slot_is_expired(&fixture.stale_fleet).await);
    assert!(fixture.slot_is_expired(&fixture.cordoned_fleet).await);
    assert!(!fixture.slot_is_expired(&fixture.unknown_fleet).await);
    assert_eq!(fixture.offline_events().await, 1);

    let second = liveness.sweep().await.expect("a repeated pass converges");
    assert_eq!(second.changed, 0);
    assert_eq!(fixture.offline_events().await, 1);

    let retention = Retention::new(fixture.database.clone());
    let retained = retention.sweep().await.expect("retention sweeps real rows");
    assert!(retained.changed >= 4);
    assert!(!fixture.lease_exists(&fixture.reported_lease).await);
    assert_eq!(fixture.lease_state(&fixture.unknown_lease).await, "expired");
    assert!(!fixture.runner_event_exists(&fixture.old_event).await);

    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: String,
    stale: String,
    draining: String,
    cordoned: String,
    unknown: String,
    stale_fleet: String,
    cordoned_fleet: String,
    unknown_fleet: String,
    stale_lease: String,
    cordoned_lease: String,
    unknown_lease: String,
    reported_lease: String,
    stale_event: String,
    cordoned_event: String,
    unknown_event: String,
    reported_event: String,
    old_event: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: mint_id(),
            stale: mint_id(),
            draining: mint_id(),
            cordoned: mint_id(),
            unknown: mint_id(),
            stale_fleet: mint_id(),
            cordoned_fleet: mint_id(),
            unknown_fleet: mint_id(),
            stale_lease: mint_id(),
            cordoned_lease: mint_id(),
            unknown_lease: mint_id(),
            reported_lease: mint_id(),
            stale_event: format!("evt_{}", mint_id()),
            cordoned_event: format!("evt_{}", mint_id()),
            unknown_event: format!("evt_{}", mint_id()),
            reported_event: format!("evt_{}", mint_id()),
            old_event: mint_id(),
            lane,
        }
    }

    async fn seed(&self) {
        let now = clock::now().as_millis();
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Sweep test', $2, $2) \
               RETURNING id \
             ) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             SELECT $3::uuid, id, 'sweep', 'test', $2 FROM tenant",
        )
        .bind(&self.tenant)
        .bind(now)
        .bind(&self.workspace)
        .execute(&mut *connection)
        .await
        .expect("the tenant and workspace seed");
        self.seed_runners(&mut connection, now).await;
        self.seed_fleets(&mut connection, now).await;
        self.seed_events_and_leases(&mut connection, now).await;
    }

    async fn seed_runners(&self, connection: &mut sqlx::PgConnection, now: i64) {
        sqlx::query(
            "INSERT INTO fleet.runners \
               (id, host_id, token_hash, sandbox_tier, admin_state, labels, \
                last_seen_at, created_at, updated_at) VALUES \
             ($1::uuid, $1, $1, 'dev_none', 'active', '[]', $5, $5, $5), \
             ($2::uuid, $2, $2, 'dev_none', 'draining', '[]', $6, $5, $5), \
             ($3::uuid, $3, $3, 'dev_none', 'cordoned', '[]', $6, $5, $5), \
             ($4::uuid, $4, $4, 'dev_none', 'future_state', '[]', $6, $5, $5)",
        )
        .bind(&self.stale)
        .bind(&self.draining)
        .bind(&self.cordoned)
        .bind(&self.unknown)
        .bind(OLD)
        .bind(NEVER_SEEN.max(now))
        .execute(connection)
        .await
        .expect("the runner rows seed");
    }

    async fn seed_fleets(&self, connection: &mut sqlx::PgConnection, now: i64) {
        for (index, fleet) in [&self.stale_fleet, &self.cordoned_fleet, &self.unknown_fleet]
            .into_iter()
            .enumerate()
        {
            sqlx::query(
                "INSERT INTO core.fleets \
                   (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                    status, created_at, updated_at) \
                 VALUES ($1::uuid, $2::uuid, $3::uuid, $4, '# test', '{}', \
                         'active', $5, $5)",
            )
            .bind(fleet)
            .bind(&self.workspace)
            .bind(&self.tenant)
            .bind(format!("sweep-{index}"))
            .bind(now)
            .execute(&mut *connection)
            .await
            .expect("the fleet row seeds");
        }
    }

    async fn seed_events_and_leases(&self, connection: &mut sqlx::PgConnection, now: i64) {
        let rows = [
            (
                &self.stale,
                &self.stale_fleet,
                &self.stale_lease,
                &self.stale_event,
                "active",
            ),
            (
                &self.cordoned,
                &self.cordoned_fleet,
                &self.cordoned_lease,
                &self.cordoned_event,
                "active",
            ),
            (
                &self.unknown,
                &self.unknown_fleet,
                &self.unknown_lease,
                &self.unknown_event,
                "active",
            ),
            (
                &self.stale,
                &self.stale_fleet,
                &self.reported_lease,
                &self.reported_event,
                "reported",
            ),
        ];
        for (runner, fleet, lease, event, status) in rows {
            self.seed_lease(
                connection,
                LeaseSeed {
                    runner,
                    fleet,
                    lease,
                    event,
                    status,
                    now,
                },
            )
            .await;
        }
        sqlx::query(
            "INSERT INTO fleet.runner_events \
               (id, runner_id, event_type, metadata, created_at) \
             VALUES ($1::uuid, $2::uuid, 'lease_acquired', '{}', $3)",
        )
        .bind(&self.old_event)
        .bind(&self.stale)
        .bind(OLD)
        .execute(connection)
        .await
        .expect("the aged per-work event seeds");
    }

    async fn seed_lease(&self, connection: &mut sqlx::PgConnection, row: LeaseSeed<'_>) {
        sqlx::query(
            "WITH event AS ( \
               INSERT INTO core.fleet_events \
               (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, created_at, updated_at) \
               VALUES ($1::uuid, $2::uuid, $3, 'test', 'chat', 'received', '{}', $4, $4) \
               RETURNING event_id \
             ) \
             INSERT INTO fleet.runner_leases \
               (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor, event_type, \
                event_created_at, posture, provider, model, metered_input_tokens, metered_cached_tokens, \
                metered_output_tokens, last_metered_at, fencing_token, lease_expires_at, status, created_at, updated_at) \
             SELECT $5::uuid, $6::uuid, $1::uuid, $2::uuid, $7::uuid, event_id, \
                    'test', 'chat', $4, 'platform', 'test', 'test', 0, 0, 0, $4, \
                    1, $4, $8, $4, $4 FROM event",
        )
        .bind(row.fleet)
        .bind(&self.workspace)
        .bind(row.event)
        .bind(OLD)
        .bind(row.lease)
        .bind(row.runner)
        .bind(&self.tenant)
        .bind(row.status)
        .execute(&mut *connection)
        .await
        .expect("the event and lease seed");
        if row.status == "active" {
            sqlx::query(
                "INSERT INTO fleet.runner_affinity \
                   (fleet_id, last_runner_id, fencing_seq, leased_until, metered_input_tokens, \
                    metered_cached_tokens, metered_output_tokens, last_metered_at, created_at, updated_at) \
                 VALUES ($1::uuid, $2::uuid, 1, $3, 0, 0, 0, $4, $4, $4)",
            )
            .bind(row.fleet)
            .bind(row.runner)
            .bind(row.now.saturating_add(60_000))
            .bind(OLD)
            .execute(connection)
            .await
            .expect("the affinity slot seeds");
        }
    }

    async fn scalar<T>(&self, query: &'static str, value: &str) -> T
    where
        T: for<'row> sqlx::Decode<'row, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
    {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(query)
            .bind(value)
            .fetch_one(&mut *connection)
            .await
            .expect("the scoped assertion reads")
    }

    async fn runner_state(&self, runner: &str) -> String {
        self.scalar(
            "SELECT admin_state FROM fleet.runners WHERE id = $1::uuid",
            runner,
        )
        .await
    }

    async fn slot_is_expired(&self, fleet: &str) -> bool {
        let leased_until: i64 = self
            .scalar(
                "SELECT leased_until FROM fleet.runner_affinity WHERE fleet_id = $1::uuid",
                fleet,
            )
            .await;
        leased_until < clock::now().as_millis()
    }

    async fn offline_events(&self) -> i64 {
        self.scalar(
            "SELECT count(*) FROM fleet.runner_events WHERE runner_id = $1::uuid AND event_type = 'runner_offline'",
            &self.stale,
        ).await
    }

    async fn lease_exists(&self, lease: &str) -> bool {
        self.scalar(
            "SELECT EXISTS(SELECT 1 FROM fleet.runner_leases WHERE id = $1::uuid)",
            lease,
        )
        .await
    }

    async fn lease_state(&self, lease: &str) -> String {
        self.scalar(
            "SELECT status FROM fleet.runner_leases WHERE id = $1::uuid",
            lease,
        )
        .await
    }

    async fn runner_event_exists(&self, event: &str) -> bool {
        self.scalar(
            "SELECT EXISTS(SELECT 1 FROM fleet.runner_events WHERE id = $1::uuid)",
            event,
        )
        .await
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

struct LeaseSeed<'a> {
    runner: &'a str,
    fleet: &'a str,
    lease: &'a str,
    event: &'a str,
    status: &'a str,
    now: i64,
}
