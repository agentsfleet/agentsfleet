//! The ledger question pages: a pass reaches at most one batch of stranded
//! fleets, and the next pass resumes after the last fleet the first returned.
//!
//! One more stranded fleet than a page holds is seeded, so no single pass can
//! reach them all, and two passes of one sweeper must. A page that restarted
//! from the head every pass would hand back the same batch twice and leave the
//! fleets behind it stranded for good.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_dragonfly::ReadyIndex;
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::reclaim::{BATCH_LIMIT, Reclaim};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::integration_reclaim_faults::RECLAIM_LANE;
use crate::support::connect_redis;

/// The instant every seeded lease expired at.
const LONG_AGO: Timestamp = Timestamp::constant(0, 1_000_000);

/// One tenant's fleets, each holding an `active` lease that expired long ago
/// on a runner of its own; answers the fleet ids.
///
/// Each runner takes its fleet's id, so the cleanup that removes the fleets
/// can name the runners too: runners belong to no tenant, and 101 runners left
/// behind would sit in every later liveness batch in the lane (ISO-1). They
/// beat at `now()`, so no liveness pass that runs meanwhile counts them stale.
const SEED_STRANDED: &str = "
WITH tenant AS (
    INSERT INTO identity.tenants (id, name, created_at, updated_at)
    VALUES ($1::uuid, 'Reclaim paging', $3, $3) RETURNING id
), workspace AS (
    INSERT INTO identity.workspaces (id, tenant_id, name, created_at)
    SELECT $2::uuid, id, $2, $3 FROM tenant RETURNING id, tenant_id
), fleets AS (
    INSERT INTO fleet.fleets (id, workspace_id, tenant_id, name, status, required_tags,
      memory_reads_workspace, memory_publishes_workspace, resource_version,
      created_at, updated_at)
    SELECT uuidv7(), w.id, w.tenant_id, 'paging-' || n, 'active', '{}', false, false, 1, $3, $3
      FROM workspace w, generate_series(1, $4) n
    RETURNING id, workspace_id
), runners AS (
    INSERT INTO runner.runners (id, host_id, token_hash, sandbox_tier, status, labels,
      network_policy, registry_allowlist, worker_count, last_seen_at, lease_version,
      created_at, updated_at)
    SELECT f.id, f.id::text, f.id::text, 'dev_none', 'active', '[]', 'deny_all_egress', '[]',
      1, now(), 0, now(), now()
      FROM fleets f
    RETURNING id
), events AS (
    INSERT INTO fleet.events (fleet_id, workspace_id, event_id, actor, event_type,
      status, request_json, created_at, updated_at)
    SELECT f.id, f.workspace_id, 'evt_' || f.id, 'test', 'chat', 'received', '{}', $3, $3
      FROM fleets f
    RETURNING id, fleet_id, workspace_id
)
INSERT INTO runner.leases (id, runner_id, fleet_id, workspace_id, tenant_id, fleet_event_id,
  event_created_at, receipt, posture, provider, model, metered_input_token_count,
  metered_cached_token_count, metered_output_token_count, fencing_token, lease_expires_at,
  status, posted_message_count, created_at, updated_at)
SELECT uuidv7(), r.id, e.fleet_id, e.workspace_id, $1::uuid, e.id, $3, e.id::text,
  'platform', 'test', 'test', 0, 0, 0, 1, $3, 'active', 0, $3, $3
  FROM events e JOIN runners r ON r.id = e.fleet_id
RETURNING fleet_id::text";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_second_pass_resumes_after_the_last_stranded_fleet_the_first_reached() {
    let _lane = RECLAIM_LANE.lock().await;
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let queue = connect_redis().await;
    let index = ReadyIndex::new(queue.clone());
    let seeded = Seeded::new(database.clone(), index.clone()).await;
    let stranded = &seeded.fleets;
    let reclaim = Reclaim::new(database.clone(), queue.clone(), "reclaim-paging");

    reclaim.sweep().await.expect("the first pass completes");
    let after_first = marked(&index, stranded).await;
    reclaim.sweep().await.expect("the second pass completes");
    let after_second = marked(&index, stranded).await;

    assert!(
        after_first < stranded.len(),
        "one pass reaches at most one page: {after_first} of {}",
        stranded.len()
    );
    assert_eq!(
        after_second,
        stranded.len(),
        "the second pass resumes where the first stopped and reaches the rest"
    );

    drop(seeded);
    drop(database);
    lane.cleanup().await;
}

/// Removes the tenant's fleets, leases and events by cascade.
const DELETE_TENANT: &str = "DELETE FROM identity.tenants WHERE id = $1::uuid";

/// Removes the runners, which carry their fleets' ids.
const DELETE_RUNNERS: &str = "DELETE FROM runner.runners WHERE id = ANY($1::uuid[])";

/// One page and one more of stranded fleets under a tenant of this run's own,
/// removed with their readiness marks when this drops, whether or not the
/// test reached its end.
struct Seeded {
    database: Db,
    index: ReadyIndex,
    tenant: String,
    fleets: Vec<String>,
}

impl Seeded {
    async fn new(database: Db, index: ReadyIndex) -> Self {
        let tenant = mint_id();
        let mut connection = database.acquire().await.expect("a connection");
        let fleets = sqlx::query_scalar(SEED_STRANDED)
            .bind(&tenant)
            .bind(mint_id())
            .bind(LONG_AGO.to_sqlx())
            .bind(i32::try_from(BATCH_LIMIT + 1).expect("one page and one more fits"))
            .fetch_all(&mut *connection)
            .await
            .expect("the stranded fleets seed");
        drop(connection);
        Self {
            database,
            index,
            tenant,
            fleets,
        }
    }
}

impl Drop for Seeded {
    /// Async cleanup from a synchronous drop: `block_in_place` hands this
    /// worker's other tasks away while the cleanup runs, which is why the test
    /// runs on the multi-threaded runtime.
    fn drop(&mut self) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                for fleet in &self.fleets {
                    let _cleared = self.index.force_clear(fleet).await;
                }
                if let Ok(mut connection) = self.database.acquire().await {
                    let _removed = sqlx::query(DELETE_TENANT)
                        .bind(&self.tenant)
                        .execute(&mut *connection)
                        .await;
                    let _retired = sqlx::query(DELETE_RUNNERS)
                        .bind(&self.fleets)
                        .execute(&mut *connection)
                        .await;
                }
            });
        });
    }
}

/// How many of `fleets` the readiness index holds a mark for.
async fn marked(index: &ReadyIndex, fleets: &[String]) -> usize {
    let mut held = 0;
    for fleet in fleets {
        if index
            .token_for(fleet)
            .await
            .expect("the readiness index is readable")
            .is_some()
        {
            held += 1;
        }
    }
    held
}
