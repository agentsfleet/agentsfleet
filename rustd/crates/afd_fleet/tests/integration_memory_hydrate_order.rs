//! Which live lease a hydrate trusts when one runner briefly holds two.
//!
//! A reclaim can leave the superseded lease and its successor both live on one
//! runner for a moment. The fence a hydrate reads must be the successor's, by
//! fencing token: tokens rise with every issue, where `created_at` is one
//! daemon replica's clock, and a replica running behind stamps the successor
//! EARLIER. Read by the clock, the hydrate would take the superseded token,
//! find it below the live sequence, and refuse the runner that holds the fleet.
//!
//! Marked `#[ignore]` so only `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::queue;
use crate::report_seed;

use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;

use self::report_seed::held;

/// How far before its predecessor the successor is stamped: any skew serves.
const CLOCK_SKEW_MS: i64 = 1_000;

/// Copies the held lease as its successor — the fleet's next token, stamped
/// [`CLOCK_SKEW_MS`] before the held lease — and moves the live sequence to it.
const INSERT_EARLIER_SUCCESSOR: &str = "\
INSERT INTO fleet.runner_leases \
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, receipt, actor, event_type, \
   event_created_at, posture, provider, model, metered_input_tokens, \
   metered_cached_tokens, metered_output_tokens, last_metered_at, fencing_token, \
   lease_expires_at, status, created_at, updated_at) \
SELECT $2::uuid, runner_id, fleet_id, workspace_id, tenant_id, event_id, receipt, actor, \
   event_type, event_created_at, posture, provider, model, 0, 0, 0, last_metered_at, \
   fencing_token + 1, lease_expires_at, status, created_at - $3, updated_at \
FROM fleet.runner_leases WHERE id = $1::uuid \
RETURNING fencing_token";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_hydrate_trusts_the_higher_token_over_the_clock() {
    let run = held().await;
    let fleet_id = Uuid7::parse(&run.fleet).expect("the fixture id is a v7 spelling");
    let plane = run.fixtures.plane();

    // Positive control: the lone held lease hydrates.
    plane
        .hydrate(&run.runner, &fleet_id, run.now)
        .await
        .expect("the held lease is the live one");

    issue_earlier_successor(&run).await;
    plane
        .hydrate(&run.runner, &fleet_id, run.now)
        .await
        .expect("the successor's token is the live sequence, whatever its clock says");

    queue::clear_ready(run.fixtures.queue(), &run.fleet).await;
    run.fixtures.cleanup().await;
}

/// Inserts the successor and raises the fleet's live sequence to its token,
/// as a won claim does.
async fn issue_earlier_successor(run: &report_seed::Held) {
    let mut connection = run
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let token: i64 = sqlx::query_scalar(INSERT_EARLIER_SUCCESSOR)
        .bind(run.issued.lease_id.as_str())
        .bind(mint_id())
        .bind(CLOCK_SKEW_MS)
        .fetch_one(&mut *connection)
        .await
        .expect("the successor lease inserts");
    assert!(
        token > run.fence.as_i64(),
        "the successor must outrank the held lease, or this test proves nothing"
    );
    sqlx::query("UPDATE fleet.runner_affinity SET fencing_seq = $2 WHERE fleet_id = $1::uuid")
        .bind(&run.fleet)
        .bind(token)
        .execute(&mut *connection)
        .await
        .expect("the live sequence moves to the successor");
}
