//! A fleet's schedule write proves its run on the write's own transaction.
//!
//! The handler's standing check lets go before the write; a reclaim landing
//! between the two would let the old holder write. `WriteFence` proves the
//! run again, under a shared lock on its row, on the transaction that writes.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::Precondition as _;
use afd_fleet::run::write_fence::WriteFence;
use agentsfleetd::supervisor::Supervisor;

use crate::schedules::Assigned;
use crate::verbs::FakeQStash;

/// Greptile on #731: an old holder resumed after a reclaim must not write.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_write_fence_holds_only_for_the_live_holder() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let assigned = Assigned::boot(&mut supervisor, &qstash).await;
    let run = assigned.run_id.clone();
    let runner = assigned.scene.runner_id.clone();
    let held = WriteFence::new(runner.clone(), run.clone(), assigned.fence);
    let stale = WriteFence::new(runner, run, assigned.fence + 1);
    let mut connection = assigned
        .scene
        .booted
        .database
        .acquire()
        .await
        .expect("a connection");

    assert!(held.holds(&mut connection).await.expect("the read answers"));
    assert!(
        !stale
            .holds(&mut connection)
            .await
            .expect("the read answers"),
        "a token the run was not issued does not hold"
    );

    // A reclaim moves the fleet's live sequence past the old holder's token.
    sqlx::query("UPDATE thread.threads SET fencing_token = fencing_token + 1 WHERE fleet_id = $1")
        .bind(&assigned.scene.fleet)
        .execute(&mut *connection)
        .await
        .expect("the reclaim lands");
    assert!(
        !held.holds(&mut connection).await.expect("the read answers"),
        "the superseded holder no longer holds"
    );
    drop(connection);
    assigned.finish(supervisor).await;
}
