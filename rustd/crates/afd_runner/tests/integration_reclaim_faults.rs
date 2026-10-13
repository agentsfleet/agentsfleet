//! The reclaim sweep's two recovery writes: claiming an entry a dead consumer
//! left pending, and re-marking a fleet whose lease a dead runner never
//! finished — the second against an index that will not take the mark.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::streams::{FLEET_CONSUMER_GROUP, fleet_stream_key};
use afd_dragonfly::{Dragonfly, FleetStreams, Partition, ReadyIndex};
use afd_runner::sql::LEASE_STATUS_ACTIVE;
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::reclaim::Reclaim;

use crate::support::{Recorder, connect_redis};

/// Held by every test that runs a reclaim pass: a pass sweeps every active
/// fleet in the lane, so two passes in parallel claim each other's entries.
/// The liveness pass holds it too, because it expires the abandoned lease a
/// reclaim fixture strands.
pub(crate) static RECLAIM_LANE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A consumer no process reads under any more.
const DEAD_CONSUMER: &str = "agentsfleetd-dead-before-reclaim";

/// Past `AUTOCLAIM_MIN_IDLE_MS`, so the sweep may take the entry.
const IDLE_PAST_THRESHOLD_MS: u64 = 400_000;

/// A Dragonfly nobody is listening on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// The instant the fixture's lease expired at.
const LONG_AGO: i64 = 1;

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_entry_a_dead_consumer_left_idle_is_claimed_and_its_fleet_marked() {
    let _lane = RECLAIM_LANE.lock().await;
    let fixture = Fixture::seed(false).await;
    let queue = connect_redis().await;
    let streams = FleetStreams::new(queue.clone());
    streams.ensure_group(&fixture.fleet).await.expect("group");
    let receipt = streams
        .append(&fixture.fleet, &[("type", "reclaim-fixture")])
        .await
        .expect("the entry appends");
    streams
        .read_new(&fixture.fleet, DEAD_CONSUMER)
        .await
        .expect("the dead consumer reads")
        .expect("and is handed the entry");
    age(&queue, &fixture.fleet, receipt.as_str()).await;
    let sweeper = format!("reclaim-{}", fixture.fleet);

    Reclaim::new(fixture.database.clone(), queue.clone(), sweeper.as_str())
        .sweep()
        .await
        .expect("the pass completes");

    assert_eq!(
        pending_consumer(&queue, &fixture.fleet).await.as_deref(),
        Some(sweeper.as_str()),
        "the stranded entry now sits with a consumer that reads"
    );
    let index = ReadyIndex::new(queue.clone());
    assert!(
        hinted(&index, &fixture.fleet).await,
        "a claimed entry is deliverable, so its fleet is marked, and hinted so the next poll takes it"
    );
    let _cleared = index.force_clear(&fixture.fleet).await;
    let _gone = streams.forget(&fixture.fleet).await;
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stranded_lease_the_index_will_not_mark_is_logged_and_not_counted() {
    let _lane = RECLAIM_LANE.lock().await;
    let fixture = Fixture::seed(true).await;
    let dead = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket");
    let recorder = Recorder::default();

    let swept = recorder
        .around(Reclaim::new(fixture.database.clone(), dead, "reclaim-dead").sweep())
        .await
        .expect("a queue that will not answer fails marks, never the pass");

    assert_eq!(swept.changed, 0, "a mark that did not land is not counted");
    let logged = recorder
        .find("ready_remark_failed", |fields| {
            fields.get("fleet_id") == Some(&fixture.fleet)
        })
        .expect("the ledger's stranded fleet is named when its mark fails");
    assert!(
        logged
            .get("error_code")
            .is_some_and(|code| !code.is_empty())
    );
    let reason = logged.get("reason").expect("the failed mark says why");
    assert!(!reason.contains("[UZ-"), "the code is repeated: {reason}");
    fixture.cleanup().await;
}

/// The ledger question marks a stranded fleet ready and flips nothing: the
/// re-lease path reads the expired lease from PostgreSQL alone, so the lease
/// must still be `active` after the pass that found it.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_stranded_lease_is_marked_ready_and_left_active() {
    let _lane = RECLAIM_LANE.lock().await;
    let fixture = Fixture::seed(true).await;
    let queue = connect_redis().await;

    Reclaim::new(fixture.database.clone(), queue.clone(), "reclaim-ledger")
        .sweep()
        .await
        .expect("the pass completes");

    let index = ReadyIndex::new(queue.clone());
    assert!(
        hinted(&index, &fixture.fleet).await,
        "the ledger's stranded fleet is marked ready, and hinted so the next poll takes it"
    );
    assert_eq!(
        fixture.lease_statuses().await,
        [LEASE_STATUS_ACTIVE],
        "the pass that found the expired lease left it for the re-lease path"
    );
    let _cleared = index.force_clear(&fixture.fleet).await;
    fixture.cleanup().await;
}

/// Whether `fleet` is marked and the directory names its partition under that
/// mark or a later one: a sibling marked since may have rewritten the entry,
/// never with an earlier token.
async fn hinted(index: &ReadyIndex, fleet: &str) -> bool {
    let Some(token) = index.token_for(fleet).await.expect("readable") else {
        return false;
    };
    let partition = Partition::of(fleet);
    index
        .dirty()
        .await
        .expect("the directory is readable")
        .iter()
        .any(|entry| entry.partition == partition && entry.token >= token)
}

/// Sets the entry's idle clock past the sweep's threshold, as time would.
async fn age(queue: &Dragonfly, fleet: &str, receipt: &str) {
    let key = fleet_stream_key(fleet);
    let mut claim = redis::cmd("XCLAIM");
    claim
        .arg(&key)
        .arg(FLEET_CONSUMER_GROUP)
        .arg(DEAD_CONSUMER)
        .arg(0)
        .arg(receipt)
        .arg("IDLE")
        .arg(IDLE_PAST_THRESHOLD_MS);
    let _: redis::Value = queue
        .command("XCLAIM", &key, &claim)
        .await
        .expect("the idle clock is set");
}

/// The consumer holding the fleet's one pending entry.
async fn pending_consumer(queue: &Dragonfly, fleet: &str) -> Option<String> {
    let key = fleet_stream_key(fleet);
    let mut pending = redis::cmd("XPENDING");
    pending
        .arg(&key)
        .arg(FLEET_CONSUMER_GROUP)
        .arg("-")
        .arg("+")
        .arg(1);
    let reply: redis::streams::StreamPendingCountReply = queue
        .command("XPENDING", &key, &pending)
        .await
        .expect("the pending list reads");
    reply.ids.into_iter().next().map(|entry| entry.consumer)
}

/// One active fleet swept first (its `updated_at` is the smallest there is),
/// optionally holding a lease whose runner never came back.
struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    fleet: String,
}

impl Fixture {
    async fn seed(stranded: bool) -> Self {
        let lane = TestDatabase::shared();
        let fixture = Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            fleet: mint_id(),
            lane,
        };
        let workspace = mint_id();
        let mut connection = fixture.database.acquire().await.expect("a connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Reclaim faults', 1, 1) RETURNING id \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               SELECT $2::uuid, id, $2, 'test', 1 FROM tenant RETURNING id, tenant_id \
             ) \
             INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, \
               config_json, status, created_at, updated_at) \
             SELECT $3::uuid, id, tenant_id, $3, '# fixture', '{}'::jsonb, 'active', 1, 1 \
             FROM workspace",
        )
        .bind(&fixture.tenant)
        .bind(&workspace)
        .bind(&fixture.fleet)
        .execute(&mut *connection)
        .await
        .expect("the fleet seeds");
        if stranded {
            fixture.strand(&mut connection, &workspace).await;
        }
        drop(connection);
        fixture
    }

    /// A runner, and an `active` lease of it that expired long ago.
    async fn strand(&self, connection: &mut sqlx::PgConnection, workspace: &str) {
        let (runner, lease, event) = (mint_id(), mint_id(), format!("evt_{}", mint_id()));
        sqlx::query(
            "WITH runner AS ( \
               INSERT INTO fleet.runners (id, host_id, token_hash, sandbox_tier, admin_state, \
                 labels, last_seen_at, created_at, updated_at) \
               VALUES ($1::uuid, $1, $1, 'dev_none', 'active', '[]', $6, $6, $6) RETURNING id \
             ), event AS ( \
               INSERT INTO core.fleet_events (fleet_id, workspace_id, event_id, actor, \
                 event_type, status, request_json, created_at, updated_at) \
               VALUES ($2::uuid, $3::uuid, $4, 'test', 'chat', 'received', '{}', $6, $6) \
               RETURNING event_id \
             ) \
             INSERT INTO fleet.runner_leases (id, runner_id, fleet_id, workspace_id, tenant_id, \
               event_id, actor, event_type, event_created_at, posture, provider, model, \
               metered_input_tokens, metered_cached_tokens, metered_output_tokens, \
               last_metered_at, fencing_token, lease_expires_at, status, created_at, \
               updated_at, receipt) \
             SELECT $5::uuid, runner.id, $2::uuid, $3::uuid, $7::uuid, event.event_id, 'test', \
               'chat', $6, 'platform', 'test', 'test', 0, 0, 0, $6, 1, $6, 'active', $6, $6, \
               event.event_id FROM runner CROSS JOIN event",
        )
        .bind(&runner)
        .bind(&self.fleet)
        .bind(workspace)
        .bind(&event)
        .bind(&lease)
        .bind(LONG_AGO)
        .bind(&self.tenant)
        .execute(connection)
        .await
        .expect("the stranded lease seeds");
    }

    /// Every lease status the fixture's fleet holds.
    async fn lease_statuses(&self) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("a connection");
        sqlx::query_scalar("SELECT status FROM fleet.runner_leases WHERE fleet_id = $1::uuid")
            .bind(&self.fleet)
            .fetch_all(&mut *connection)
            .await
            .expect("the lease statuses read")
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("a connection");
        let _removed = sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await;
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
