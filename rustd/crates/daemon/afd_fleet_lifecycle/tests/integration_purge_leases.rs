//! A purge takes its fleet's live leases and keeps its settled ones.
//!
//! `runner.leases` holds no foreign key to the fleet, so nothing cascades: the
//! purge deletes the live lease itself, because one left behind would name a
//! fleet that no longer exists to the renew and report paths. The settled lease
//! stays as the runner's history, for its own retention window.
//!
//! `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;

use crate::support::{Lane, mint};

/// One runner, one event on `$3`, and two leases of it: `$5` live, `$6` settled.
/// `$1` tenant · `$2` workspace · `$3` fleet · `$4` runner · `$7` event id ·
/// `$8` host · `$9` token hash.
const SEED: &str = r"
WITH runner AS (
    INSERT INTO runner.runners
      (id, host_id, token_hash, sandbox_tier, status, labels, network_policy,
       registry_allowlist, worker_count, lease_version, last_seen_at, created_at, updated_at)
    VALUES ($4, $8, $9, 'dev_none', 'active', '[]'::jsonb, 'deny_all_egress',
            '[]'::jsonb, 1, 0, now(), now(), now())
), event AS (
    INSERT INTO fleet.events
      (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, wall_ms,
       failure_reason, failure_detail, created_at, updated_at)
    VALUES ($3, $2, $7, 'user:a', 'chat', 'processed', '{}'::jsonb, 900,
            NULL, NULL, now(), now())
)
INSERT INTO runner.leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, event_created_at,
   posture, provider, model, last_metered_at, fencing_token, lease_expires_at,
   status, posted_message_count,
   created_at, updated_at)
VALUES
  ($5, $4, $3, $2, $1, $7, now(), 'platform',
   'anthropic', 'model-a', now(), 2, now(), 'active', 0, now(), now()),
  ($6, $4, $3, $2, $1, $7, now(), 'platform',
   'anthropic', 'model-a', now(), 1, now(), 'reported', 0, now(), now())
";

/// Whether a lease row still exists.
const LEASE_EXISTS: &str = "SELECT EXISTS (SELECT 1 FROM runner.leases WHERE id = $1)";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_purge_deletes_the_live_lease_and_keeps_the_settled_one() {
    let lane = Lane::create().await;
    let fleet = lane.fleet().await;
    let (live, settled) = (mint(), mint());
    sqlx::query(SEED)
        .bind(&lane.tenant)
        .bind(&lane.workspace)
        .bind(&fleet)
        .bind(mint())
        .bind(&live)
        .bind(&settled)
        .bind(mint())
        .bind(format!("host-{live}"))
        .bind(format!("hash-{live}"))
        .execute(&mut *lane.connection().await)
        .await
        .expect("the runner, event and leases seed");

    let tag = lane.kill(&fleet).await;
    lane.fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
        .await
        .expect("a killed fleet purges under its current tag");

    assert!(
        !exists(&lane, &live).await,
        "a live lease goes with its fleet"
    );
    assert!(
        exists(&lane, &settled).await,
        "a settled lease stays as the runner's history"
    );

    lane.cleanup().await;
}

/// Whether `lease` survived.
async fn exists(lane: &Lane, lease: &Uuid7) -> bool {
    sqlx::query_scalar(LEASE_EXISTS)
        .bind(lease)
        .fetch_one(&mut *lane.connection().await)
        .await
        .expect("the lane answers")
}
