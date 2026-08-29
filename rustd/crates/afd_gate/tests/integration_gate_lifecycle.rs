//! Gate parking and durable-decision fallback over both live datastores.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::Mode;
use afd_fleet_runtime::provider::StaticRegistry;
use afd_gate::gate::{Check, Gates, Refused, Trigger, Verdict, Waiting};
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use sqlx::Acquire as _;

const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);
const REDIS_URL_KNOB: &str = "TEST_REDIS_URL";
const REDIS_CA_KNOB: &str = "TEST_REDIS_CA_CERT";

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn write_gate_parks_once_and_honours_each_durable_outcome() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let writing = config(true);

    assert_approved_path(&gates, &fixture, &writing).await;
    assert_denied_path(&gates, &fixture, &writing).await;
    assert_expired_path(&gates, &fixture, &writing).await;
    assert_eq!(
        gates
            .check(fixture.check("event-free", &config(false)), NOW)
            .await,
        Verdict::Pass
    );
    fixture.cleanup().await;
}

async fn assert_approved_path(gates: &Gates, fixture: &Fixture, writing: &FleetConfig) {
    assert_eq!(
        gates
            .check(fixture.check("event-approved", writing), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );
    assert_eq!(
        gates
            .check(fixture.check("event-approved", writing), NOW)
            .await,
        Verdict::Await(Waiting::Pending)
    );
    fixture.resolve("event-approved", "approved").await;
    assert_eq!(
        gates
            .check(fixture.check("event-approved", writing), NOW)
            .await,
        Verdict::Pass
    );
}

async fn assert_denied_path(gates: &Gates, fixture: &Fixture, writing: &FleetConfig) {
    assert_eq!(
        gates
            .check(fixture.check("event-denied", writing), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );
    fixture.resolve("event-denied", "denied").await;
    assert_eq!(
        gates
            .check(fixture.check("event-denied", writing), NOW)
            .await,
        Verdict::Refuse(Refused::Denied)
    );
}

async fn assert_expired_path(gates: &Gates, fixture: &Fixture, writing: &FleetConfig) {
    assert_eq!(
        gates
            .check(fixture.check("event-expired", writing), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );
    assert_eq!(
        gates
            .check(
                fixture.check("event-expired", writing),
                NOW.saturating_add_millis(3_600_001),
            )
            .await,
        Verdict::Refuse(Refused::Expired)
    );
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn authored_rules_and_anomaly_thresholds_drive_each_first_encounter_route() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );

    let approval = config_gates(
        r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"approve","gate_kind":"deploy","blast_radius":"production"}]}"#,
    );
    assert_eq!(
        gates
            .check(fixture.check("event-rule-approval", &approval), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );

    let policy_kill = config_gates(
        r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"auto_kill"}]}"#,
    );
    assert_eq!(
        gates
            .check(fixture.check("event-policy-kill", &policy_kill), NOW)
            .await,
        Verdict::Killed(Trigger::Policy)
    );
    fixture.activate().await;

    let anomaly = config_gates(
        r#"{"anomaly_rules":[{"pattern":"same_action","threshold_count":2,"threshold_window_s":60}]}"#,
    );
    assert_eq!(
        gates
            .check(fixture.check("event-anomaly-first", &anomaly), NOW)
            .await,
        Verdict::Pass
    );
    assert_eq!(
        gates
            .check(fixture.check("event-anomaly-second", &anomaly), NOW)
            .await,
        Verdict::Killed(Trigger::Anomaly)
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_gate_that_cannot_mint_its_identity_fails_closed() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let (entropy, control) = Entropy::new_mocked();
    control.fail_next();
    let gates = Gates::new(fixture.database.clone(), connect_redis().await, entropy);

    assert_eq!(
        gates
            .check(fixture.check("event-entropy-failure", &config(true)), NOW)
            .await,
        Verdict::Unavailable,
        "a gate with no durable identity cannot release the event"
    );
    fixture.cleanup().await;
}

#[test]
fn waiting_states_have_stable_operator_names() {
    assert_eq!(Waiting::Parked.as_str(), "parked");
    assert_eq!(Waiting::Pending.as_str(), "pending");
    assert_eq!(Waiting::Unreadable.as_str(), "unreadable");
}

fn config(repository_write: bool) -> FleetConfig {
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

fn config_gates(gates: &str) -> FleetConfig {
    let document = format!(
        r#"{{"name":"gate-fixture","x-agentsfleet":{{"triggers":[{{"type":"api"}}],"tools":[],"budget":{{"daily_dollars":1.0}},"gates":{gates}}}}}"#
    );
    FleetConfig::parse(&document, Mode::Stored, &StaticRegistry::default())
        .expect("the gate policy fixture resolves")
}

async fn connect_redis() -> Redis {
    let url = std::env::var(REDIS_URL_KNOB)
        .expect("TEST_REDIS_URL is set by make test-integration-rustd");
    let config = RedisConfig::from_url(RedisRole::Default, url)
        .with_ca_cert_file(std::env::var(REDIS_CA_KNOB).ok().map(Into::into))
        .with_connect_timeout(Duration::from_secs(5))
        .with_request_timeout(Duration::from_secs(5));
    afd_redis::test_util::connect_live(&config)
        .await
        .expect("the lane's Redis must be reachable")
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: Uuid7,
    workspace: Uuid7,
    fleet: Uuid7,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: id(),
            workspace: id(),
            fleet: id(),
            lane,
        }
    }

    fn check<'fixture>(
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

    async fn seed(&self) {
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

    async fn resolve(&self, event_id: &str, status: &str) {
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

    async fn activate(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("UPDATE core.fleets SET status = 'active' WHERE id = $1::uuid")
            .bind(self.fleet.as_str())
            .execute(&mut *connection)
            .await
            .expect("the fixture fleet is reactivated");
    }

    async fn cleanup(self) {
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
