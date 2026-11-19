//! A purge takes its fleet's live runs and keeps its settled ones.
//!
//! `thread.runs` holds no foreign key to the fleet, so nothing cascades: the
//! purge deletes the live run itself, because one left behind would name a
//! fleet that no longer exists to the renew and report paths. The settled run
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

/// One runner, one event on `$3`, and two runs of it: `$5` live, `$6` settled.
/// `$1` tenant · `$2` workspace · `$3` fleet · `$4` runner · `$7` event id ·
/// `$8` host · `$9` token hash.
const SEED: &str = r"
WITH runner AS (
    INSERT INTO runner.runners
      (id, host_id, token_hash, sandbox_tier, status, labels, network_policy,
       registry_allowlist, worker_count, run_version, last_seen_at, created_at, updated_at,
       platform_owned)
    VALUES ($4, $8, $9, 'dev_none', 'active', '[]'::jsonb, 'deny_all_egress',
            '[]'::jsonb, 1, 0, now(), now(), now(), true)
), event AS (
    INSERT INTO thread.turns
      (thread_id, workspace_id, id, actor, kind, status, request_json, wall_ms,
       failure_reason, failure_detail, opened_at, settled_at, created_at, updated_at)
    VALUES ($3, $2, $7, 'user:a', 'chat', 'processed', '{}'::jsonb, 900,
            NULL, NULL, now(), now(), now(), now())
)
INSERT INTO thread.runs
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, event_created_at,
   posture, provider, model, last_metered_at, fencing_token, run_expires_at,
   status, posted_message_count,
   created_at, updated_at)
VALUES
  ($5, $4, $3, $2, $1, $7, now(), 'platform',
   'anthropic', 'model-a', now(), 2, now(), 'active', 0, now(), now()),
  ($6, $4, $3, $2, $1, $7, now(), 'platform',
   'anthropic', 'model-a', now(), 1, now(), 'reported', 0, now(), now())
";

/// Whether a run row still exists.
const RUN_EXISTS: &str = "SELECT EXISTS (SELECT 1 FROM thread.runs WHERE id = $1)";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_purge_deletes_the_live_run_and_keeps_the_settled_one() {
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
        .expect("the runner, event and runs seed");

    lane.archive(&fleet).await;
    lane.delete(&lane.workspace, &fleet)
        .await
        .expect("an archived fleet purges under its current tag");

    assert!(
        !exists(&lane, &live).await,
        "a live run goes with its fleet"
    );
    assert!(
        exists(&lane, &settled).await,
        "a settled run stays as the runner's history"
    );

    lane.cleanup().await;
}

/// Whether `run` survived.
async fn exists(lane: &Lane, run: &Uuid7) -> bool {
    sqlx::query_scalar(RUN_EXISTS)
        .bind(run)
        .fetch_one(&mut *lane.connection().await)
        .await
        .expect("the lane answers")
}
