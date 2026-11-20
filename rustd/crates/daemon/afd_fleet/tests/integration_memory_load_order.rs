//! Which live run a load trusts when one runner briefly holds two.
//!
//! A reassignment can leave the superseded run and its successor both live on one
//! runner for a moment. The fence a load reads must be the successor's, by
//! fencing token: tokens rise with every issue, where `created_at` is one
//! daemon replica's clock, and a replica running behind stamps the successor
//! EARLIER. Read by the clock, the load would take the superseded token,
//! find it below the live sequence, and refuse the runner that holds the fleet.
//!
//! Marked `#[ignore]` so only `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::report_seed;

use afd_db::test_util::mint_id;

use self::report_seed::held;

/// How far before its predecessor the successor is stamped: any skew serves.
const CLOCK_SKEW: std::time::Duration = std::time::Duration::from_secs(1);

/// Copies the held run as its successor — the fleet's next token, stamped
/// [`CLOCK_SKEW`] before the held run — and moves the live sequence to it.
const INSERT_EARLIER_SUCCESSOR: &str = "\
INSERT INTO thread.runs \
  (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at, \
   posture, provider, model, last_metered_at, fencing_token, \
   run_expires_at, status, posted_message_count, created_at, updated_at) \
SELECT $2, runner_id, fleet_id, workspace_id, tenant_id, turn_id, \
   turn_created_at, posture, provider, model, last_metered_at, \
   fencing_token + 1, run_expires_at, status, 0, created_at - $3::interval, updated_at \
FROM thread.runs WHERE id = $1 \
RETURNING fencing_token";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_load_trusts_the_higher_token_over_the_clock() {
    let run = held().await;
    let fleet_id = run.fleet.clone();
    let plane = run.fixtures.plane();

    // Positive control: the lone held run loads.
    plane
        .load(&run.runner, &fleet_id)
        .await
        .expect("the held run is the live one");

    issue_earlier_successor(&run).await;
    plane
        .load(&run.runner, &fleet_id)
        .await
        .expect("the successor's token is the live sequence, whatever its clock says");

    run.fixtures.cleanup().await;
}

/// Inserts the successor and raises the fleet's live sequence to its token,
/// as a won assignment does.
async fn issue_earlier_successor(run: &report_seed::Held) {
    let mut connection = run
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let token: i64 = sqlx::query_scalar(INSERT_EARLIER_SUCCESSOR)
        .bind(&run.issued.run_id)
        .bind(mint_id())
        .bind(CLOCK_SKEW)
        .fetch_one(&mut *connection)
        .await
        .expect("the successor run inserts");
    assert!(
        token > run.fence.as_i64(),
        "the successor must outrank the held run, or this test proves nothing"
    );
    sqlx::query("UPDATE thread.threads SET fencing_token = $2 WHERE fleet_id = $1")
        .bind(&run.fleet)
        .bind(token)
        .execute(&mut *connection)
        .await
        .expect("the live sequence moves to the successor");
}
