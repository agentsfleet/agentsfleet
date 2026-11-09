//! The scope a gate test parks in: a tenant, a workspace, a fleet, and the
//! Postgres and NATS the gate plane needs. Shared by the lifecycle suite and the
//! live-tail suite, so each compiles its own copy and uses a subset.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_fleet_runtime::FleetConfig;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_fleet_lifecycle::FleetStatus;
use afd_gate::gate::Check;
use jiff::Timestamp;

/// The instant a park mints its identifiers at. Only identifiers: every stamp
/// and deadline on a gate is the database's.
pub(crate) const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

pub(crate) fn config(repository_write: bool) -> FleetConfig {
    let repository = if repository_write {
        r#", "repositories":["agentsfleet/test"], "repository_access":"write", "repository_base":"main""#
    } else {
        ""
    };
    let document = format!(
        r#"{{"name":"gate-fixture","x-agentsfleet":{{"triggers":[],"tools":[],"budget":{{"daily_nanos":"1000000000"}}{repository}}}}}"#
    );
    FleetConfig::stored(&document).expect("the stored fixture config resolves")
}

pub(crate) fn config_gates(gates: &str) -> FleetConfig {
    let document = format!(
        r#"{{"name":"gate-fixture","x-agentsfleet":{{"triggers":[],"tools":[],"budget":{{"daily_nanos":"1000000000"}},"gates":{gates}}}}}"#
    );
    FleetConfig::stored(&document).expect("the gate policy fixture resolves")
}

/// The lane's NATS, which the gate plane publishes on.
///
/// One per test: clones share the connection, so a subscribe then a publish
/// stay ordered.
pub(crate) async fn bus() -> afd_bus::Handle {
    afd_bus::test_util::lane()
        .await
        .expect("the lane's NATS: run through make test-integration-rustd")
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
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
            lane,
        }
    }

    pub(crate) const fn check<'fixture>(
        &'fixture self,
        event_id: &'fixture Uuid7,
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
        TenantSeed::new(&self.tenant)
            .name("Gate lifecycle")
            .insert(&mut *connection)
            .await
            .expect("the gate scope seeds");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .insert(&mut *connection)
            .await
            .expect("the gate scope seeds");
        FleetSeed::new(
            &self.fleet,
            &self.workspace,
            &self.tenant,
            FleetStatus::Active.as_str(),
        )
        .name(self.fleet.as_str())
        .insert(&mut *connection)
        .await
        .expect("the gate scope seeds");
    }

    pub(crate) async fn resolve(&self, event_id: &Uuid7, status: &str) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let changed = sqlx::query(
            "UPDATE fleet.approval_gates \
             SET status = $3, resolved_by = 'user:reviewer', \
                 resolved_at = now(), updated_at = now() \
             WHERE id = ( \
               SELECT id FROM fleet.approval_gates \
               WHERE fleet_id = $1 AND event_id = $2 AND resolved_at IS NULL \
               ORDER BY created_at DESC, id DESC LIMIT 1 \
             )",
        )
        .bind(&self.fleet)
        .bind(event_id)
        .bind(status)
        .execute(&mut *connection)
        .await
        .expect("the reviewer decision persists");
        assert_eq!(changed.rows_affected(), 1);
    }

    /// Moves the pending gate `event_id` holds behind the database's clock, as
    /// a deadline passing would.
    pub(crate) async fn lapse(&self, event_id: &Uuid7) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let changed = sqlx::query(
            "UPDATE fleet.approval_gates \
             SET expires_at = now() - interval '1 second', updated_at = now() \
             WHERE fleet_id = $1 AND event_id = $2 AND resolved_at IS NULL",
        )
        .bind(&self.fleet)
        .bind(event_id)
        .execute(&mut *connection)
        .await
        .expect("a pending gate's deadline can move");
        assert_eq!(changed.rows_affected(), 1);
    }

    /// Every approval card this fixture's fleet holds, of any kind.
    ///
    /// Deliberately unfiltered by `gate_type`: the claim a write fleet makes is
    /// that it raises NOTHING, and a count filtered to one kind would pass
    /// while a card of another was raised.
    pub(crate) async fn card_count(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM fleet.approval_gates WHERE fleet_id = $1",
        )
        .bind(&self.fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the gate rows are readable")
    }

    pub(crate) async fn activate(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("UPDATE fleet.fleets SET status = $2, updated_at = now() WHERE id = $1")
            .bind(&self.fleet)
            .bind(FleetStatus::Active.as_str())
            .execute(&mut *connection)
            .await
            .expect("the fixture fleet is reactivated");
    }

    /// Deletes the fixture's tenant. The gates go with their fleet on the
    /// schema's cascade, which runs as the table owner — the API role holds no
    /// DELETE on `fleet.approval_gates`, and needs none here.
    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
            .bind(self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
