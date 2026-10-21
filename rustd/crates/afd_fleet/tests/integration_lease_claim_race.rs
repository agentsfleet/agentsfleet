//! A claim that loses the fleet's slot to a runner that claimed it first.
//!
//! The candidate scan reads the slot as free, because the other runner's claim
//! has not committed. This runner's claim then waits on that row, and once the
//! other commits it finds a live lease and takes nothing: no event is read, so
//! nothing is orphaned, and the poll moves on as if the fleet were never
//! offered. The other runner's claim is held open in a transaction so the wait
//! is certain rather than a race the scheduler may or may not produce.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::id::Uuid7;
use afd_core::timing::{LEASE_TTL, RUNNER_OFFLINE_AFTER};
use afd_fleet::lease::sql::lease::CLAIM_FLEET_SLOT;
use sqlx::Connection as _;

use crate::seed::{Seeded, seeded};
use crate::support::Fixtures;

/// How long the case waits for the poll to queue behind the open claim.
const QUEUED_WITHIN: Duration = Duration::from_secs(10);

/// How often it looks.
const POLL: Duration = Duration::from_millis(20);

/// Sessions queued behind `$1`.
const WAITERS: &str = "SELECT count(*) FROM pg_stat_activity \
     WHERE $1::integer = ANY(pg_blocking_pids(pid))";

/// Which runner the slot names now.
const HOLDER: &str =
    "SELECT last_runner_id::text FROM runner.fleet_slots WHERE fleet_id = $1::uuid";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_claim_that_waited_on_a_live_claim_takes_nothing() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [first, second],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let fleet_id = Uuid7::parse(&fleet).expect("a seeded fleet id");
    let mut other = fixtures.database.acquire().await.expect("a connection");
    let mut claiming = other.begin().await.expect("a transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *claiming)
        .await
        .expect("the session names itself");
    sqlx::query(CLAIM_FLEET_SLOT)
        .bind(fleet.as_str())
        .bind(second.as_str())
        .bind(LEASE_TTL)
        .bind(RUNNER_OFFLINE_AFTER)
        .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
        .fetch_optional(&mut *claiming)
        .await
        .expect("the other runner's claim wins, uncommitted");

    let poll = tokio::spawn({
        let (leases, first, named) = (fixtures.leases(), first.clone(), [fleet_id]);
        async move { leases.select(&first, &named).await }
    });
    let deadline = tokio::time::Instant::now() + QUEUED_WITHIN;
    let mut queued = 0_i64;
    while queued == 0 && tokio::time::Instant::now() < deadline {
        queued = sqlx::query_scalar(WAITERS)
            .bind(pid)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
            .await
            .expect("the waiters count");
        tokio::time::sleep(POLL).await;
    }
    claiming
        .commit()
        .await
        .expect("the other runner's claim commits");
    drop(other);
    let polled = poll
        .await
        .expect("the poll joins")
        .expect("a lost claim is not a fault");

    assert_eq!(queued, 1, "the poll's claim queued behind the open one");
    assert!(polled.is_none(), "the lost claim takes nothing: {polled:?}");
    let holder: String = sqlx::query_scalar(HOLDER)
        .bind(&fleet)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the slot reads back");
    assert_eq!(holder, second.as_str(), "the slot is the first claimer's");
    fixtures.cleanup().await;
}
