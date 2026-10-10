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

use crate::integration_reclaim_faults::RECLAIM_LANE;
use crate::support::connect_redis;

/// The instant every seeded lease expired at.
const LONG_AGO: i64 = 1;

/// One tenant's fleets, each holding an `active` lease that expired long ago
/// on a runner of its own; answers the fleet ids.
const SEED_STRANDED: &str = "
WITH tenant AS (
    INSERT INTO core.tenants (id, name, created_at, updated_at)
    VALUES ($1::uuid, 'Reclaim paging', 1, 1) RETURNING id
), workspace AS (
    INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
    SELECT $2::uuid, id, $2, 'test', 1 FROM tenant RETURNING id, tenant_id
), fleets AS (
    INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown,
      config_json, status, created_at, updated_at)
    SELECT uuidv7(), w.id, w.tenant_id, 'paging-' || n, '# fixture', '{}'::jsonb,
      'active', 1, 1
      FROM workspace w, generate_series(1, $4) n
    RETURNING id, workspace_id
), runners AS (
    INSERT INTO fleet.runners (id, host_id, token_hash, sandbox_tier, admin_state,
      labels, last_seen_at, created_at, updated_at)
    SELECT f.id, f.id::text, f.id::text, 'dev_none', 'active', '[]', $3, $3, $3
      FROM fleets f
    RETURNING id
), events AS (
    INSERT INTO core.fleet_events (fleet_id, workspace_id, event_id, actor,
      event_type, status, request_json, created_at, updated_at)
    SELECT f.id, f.workspace_id, 'evt_' || f.id, 'test', 'chat', 'received', '{}', $3, $3
      FROM fleets f
    RETURNING fleet_id, workspace_id, event_id
)
INSERT INTO fleet.runner_leases (id, runner_id, fleet_id, workspace_id, tenant_id,
  event_id, actor, event_type, event_created_at, posture, provider, model,
  metered_input_tokens, metered_cached_tokens, metered_output_tokens,
  last_metered_at, fencing_token, lease_expires_at, status, created_at,
  updated_at, receipt)
SELECT uuidv7(), r.id, e.fleet_id, e.workspace_id, $1::uuid, e.event_id, 'test',
  'chat', $3, 'platform', 'test', 'test', 0, 0, 0, $3, 1, $3, 'active', $3, $3,
  e.event_id
  FROM events e JOIN runners r ON r.id = e.fleet_id
RETURNING fleet_id::text";

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_second_pass_resumes_after_the_last_stranded_fleet_the_first_reached() {
    let _lane = RECLAIM_LANE.lock().await;
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let tenant = mint_id();
    let stranded = seed(&database, &tenant).await;
    let queue = connect_redis().await;
    let index = ReadyIndex::new(queue.clone());
    let reclaim = Reclaim::new(database.clone(), queue.clone(), "reclaim-paging");

    reclaim.sweep().await.expect("the first pass completes");
    let after_first = marked(&index, &stranded).await;
    reclaim.sweep().await.expect("the second pass completes");
    let after_second = marked(&index, &stranded).await;

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

    for fleet in &stranded {
        let _cleared = index.force_clear(fleet).await;
    }
    let mut connection = database.acquire().await.expect("a connection");
    let _removed = sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
        .bind(&tenant)
        .execute(&mut *connection)
        .await;
    drop(connection);
    drop(database);
    lane.cleanup().await;
}

/// One page and one more of stranded fleets under a tenant of their own.
async fn seed(database: &Db, tenant: &str) -> Vec<String> {
    let mut connection = database.acquire().await.expect("a connection");
    sqlx::query_scalar(SEED_STRANDED)
        .bind(tenant)
        .bind(mint_id())
        .bind(LONG_AGO)
        .bind(i32::try_from(BATCH_LIMIT + 1).expect("one page and one more fits"))
        .fetch_all(&mut *connection)
        .await
        .expect("the stranded fleets seed")
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
