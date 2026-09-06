//! The scope a gate test parks in: a tenant, a workspace, a fleet, and the
//! two datastores the gate plane needs. Shared by the lifecycle suite and the
//! live-tail suite, so each compiles its own copy and uses a subset.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::Mode;
use afd_fleet_runtime::provider::StaticRegistry;
use afd_gate::gate::Check;
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use sqlx::Acquire as _;

pub(crate) const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);
const REDIS_URL_KNOB: &str = "TEST_REDIS_URL";
const REDIS_CA_KNOB: &str = "TEST_REDIS_CA_CERT";

pub(crate) fn config(repository_write: bool) -> FleetConfig {
    let repository = if repository_write {
        r#", "repositories":["agentsfleet/test"], "repository_access":"write", "repository_base":"main""#
    } else {
        ""
    };
    let document = format!(
        r#"{{"name":"gate-fixture","x-agentsfleet":{{"triggers":[{{"type":"api"}}],"tools":[],"budget":{{"daily_dollars":1.0}}{repository}}}}}"#
    );
    FleetConfig::parse(&document, Mode::Stored, &StaticRegistry::default())
        .expect("the stored fixture config resolves")
}

pub(crate) fn config_gates(gates: &str) -> FleetConfig {
    let document = format!(
        r#"{{"name":"gate-fixture","x-agentsfleet":{{"triggers":[{{"type":"api"}}],"tools":[],"budget":{{"daily_dollars":1.0}},"gates":{gates}}}}}"#
    );
    FleetConfig::parse(&document, Mode::Stored, &StaticRegistry::default())
        .expect("the gate policy fixture resolves")
}

pub(crate) fn redis_config() -> RedisConfig {
    let url = std::env::var(REDIS_URL_KNOB)
        .expect("TEST_REDIS_URL is set by make test-integration-rustd");
    RedisConfig::from_url(RedisRole::Default, url)
        .with_ca_cert_file(std::env::var(REDIS_CA_KNOB).ok().map(Into::into))
        .with_connect_timeout(Duration::from_secs(5))
        .with_request_timeout(Duration::from_secs(5))
}

pub(crate) async fn connect_redis() -> Redis {
    afd_redis::test_util::connect_live(&redis_config())
        .await
        .expect("the lane's Redis must be reachable")
}

pub(crate) struct Fixture {
    lane: TestDatabase,
    pub(crate) database: Db,
    tenant: Uuid7,
    workspace: Uuid7,
    pub(crate) fleet: Uuid7,
}

impl Fixture {
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: id(),
            workspace: id(),
            fleet: id(),
            lane,
        }
    }

    pub(crate) fn check<'fixture>(
        &'fixture self,
        event_id: &'fixture str,
        config: &'fixture FleetConfig,
    ) -> Check<'fixture> {
        Check {
            fleet_id: &self.fleet,
            workspace_id: &self.workspace,
            event_id,
            event_type: "chat",
            actor: "user:fixture",
            request_json: r#"{"proposed_action":"update dependency","evidence":{"pr":7}}"#,
            config,
        }
    }

    pub(crate) async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Gate lifecycle', 1, 1) \
               RETURNING id \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               SELECT $2::uuid, id, $2, 'test', 1 FROM tenant \
               RETURNING id, tenant_id \
             ) \
             INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at) \
             SELECT $3::uuid, id, tenant_id, $3, '# gate', '{}', 'active', 1, 1 \
             FROM workspace",
        )
        .bind(self.tenant.as_str())
        .bind(self.workspace.as_str())
        .bind(self.fleet.as_str())
        .execute(&mut *connection)
        .await
        .expect("the gate scope seeds");
    }

    pub(crate) async fn resolve(&self, event_id: &str, status: &str) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let changed = sqlx::query(
            "UPDATE core.fleet_approval_gates \
             SET status = $3, resolved_by = 'user:reviewer', updated_at = $4 \
             WHERE id = ( \
               SELECT id FROM core.fleet_approval_gates \
               WHERE fleet_id = $1::uuid AND event_id = $2 AND status = 'pending' \
               ORDER BY created_at DESC, id DESC LIMIT 1 \
             )",
        )
        .bind(self.fleet.as_str())
        .bind(event_id)
        .bind(status)
        .bind(NOW.as_millis())
        .execute(&mut *connection)
        .await
        .expect("the reviewer decision persists");
        assert_eq!(changed.rows_affected(), 1);
    }

    pub(crate) async fn activate(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("UPDATE core.fleets SET status = 'active' WHERE id = $1::uuid")
            .bind(self.fleet.as_str())
            .execute(&mut *connection)
            .await
            .expect("the fixture fleet is reactivated");
    }

    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = connection.begin().await.expect("cleanup begins");
        sqlx::query("SET LOCAL fleet.allow_gate_purge = 'on'")
            .execute(&mut *transaction)
            .await
            .expect("the sanctioned history purge is enabled");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(self.tenant.as_str())
            .execute(&mut *transaction)
            .await
            .expect("the scoped fixture cleans up");
        transaction.commit().await.expect("cleanup commits");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

fn id() -> Uuid7 {
    Uuid7::parse(&mint_id()).expect("the minted fixture id is UUIDv7")
}
