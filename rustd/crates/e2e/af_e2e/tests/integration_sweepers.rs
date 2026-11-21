//! The supervised sweepers' first pass, against the lane: a booted daemon
//! finishes what the request path left behind before it has served anything.
//!
//! A sweeper ticks once at spawn, so seeding before boot is what puts work in
//! front of that first pass. The row is one only a sweeper ends: a replay
//! receipt whose retry window closed. A usage line needs no sweep: the insert
//! that logs it charges it. Asserted on the row rather than on a log line,
//! because a sibling suite's daemon on the same lane may take the pass first,
//! and the claim is that the row ends, whichever daemon ends it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_db::test_util::mint_id;
use agentsfleetd::serve::boot;
use agentsfleetd::supervisor::Supervisor;
use sqlx::Connection as _;

use crate::e2e::{DATABASE_LANE_KNOB, EPHEMERAL, SCENARIOS, lane};
use crate::e2e_db::daemon_environment;
use agentsfleetd::test_util::install_subscriber;

/// The scope the expired receipt was written under.
const SCOPE: &str = "sweeper-fixture";

/// How long the first pass is given; a tick is immediate, so this is slack.
const FIRST_PASS: Duration = Duration::from_secs(15);

/// How often the rows are looked at while the pass runs.
const POLL: Duration = Duration::from_millis(100);

/// A receipt two days old, past the one-day replay window.
const INSERT_EXPIRED_RECEIPT: &str = "INSERT INTO setup.replay_receipts \
     (id, workspace_id, user_id, scope, operation_id, request_digest, resource_id, created_at) \
     VALUES ($1, $2, $2, $3, $1, 'digest', $2, \
             now() - interval '2 days')";

/// Whether the receipt survives.
const LEFT_BEHIND: &str = "SELECT count(*) FROM setup.replay_receipts WHERE id = $1";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_the_first_sweep_ends_expired_receipts() {
    install_subscriber();
    let database_url = lane(DATABASE_LANE_KNOB);
    let receipt = mint_id();
    let mut seeding = sqlx::PgConnection::connect(&database_url)
        .await
        .expect("the lane's Postgres accepts a connection");
    sqlx::query(INSERT_EXPIRED_RECEIPT)
        .bind(&receipt)
        .bind(mint_id())
        .bind(SCOPE)
        .execute(&mut seeding)
        .await
        .expect("the expired receipt seeds");

    // Serialize daemons that share placement rows with the scenarios.
    let exclusive = SCENARIOS.lock().await;
    let mut supervisor = Supervisor::new();
    let booted = boot(
        &daemon_environment(&database_url, None, &[]),
        EPHEMERAL,
        &mut supervisor,
    )
    .await
    .expect("the lane's Postgres and NATS are up");

    let deadline = tokio::time::Instant::now() + FIRST_PASS;
    let mut left: i64 = -1;
    while left != 0 && tokio::time::Instant::now() < deadline {
        left = sqlx::query_scalar(LEFT_BEHIND)
            .bind(&receipt)
            .fetch_one(&mut seeding)
            .await
            .expect("the leftovers count");
        tokio::time::sleep(POLL).await;
    }
    supervisor.shutdown().await;
    drop(booted);
    drop(exclusive);

    assert_eq!(left, 0, "the first pass deletes the expired receipt");
}
