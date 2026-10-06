//! A fleet's schedule write proves its lease on the write's own transaction.
//!
//! The handler's standing check lets go before the write; a reclaim landing
//! between the two would let the old holder write. `WriteFence` proves the
//! lease again, under a shared lock on its row, on the transaction that writes.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_db::Precondition as _;
use afd_fleet::lease::write_fence::WriteFence;
use agentsfleetd::supervisor::Supervisor;

use crate::schedules::Leased;
use crate::verbs::FakeQStash;

/// Greptile on #731: an old holder resumed after a reclaim must not write.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_a_write_fence_holds_only_for_the_live_holder() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let lease = Uuid7::parse(&leased.lease_id).expect("a lease id");
    let runner = leased.run.runner_id.clone();
    let now = afd_core::clock::now();
    let held = WriteFence::new(runner.clone(), lease.clone(), leased.fence, now);
    let stale = WriteFence::new(runner, lease, leased.fence + 1, now);
    let mut connection = leased
        .run
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
        "a token the lease was not issued does not hold"
    );

    // A reclaim moves the fleet's live sequence past the old holder's token.
    sqlx::query(
        "UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1 WHERE fleet_id = $1::uuid",
    )
    .bind(&leased.run.fleet)
    .execute(&mut *connection)
    .await
    .expect("the reclaim lands");
    assert!(
        !held.holds(&mut connection).await.expect("the read answers"),
        "the superseded holder no longer holds"
    );
    drop(connection);
    leased.finish(supervisor).await;
}
