//! An assignment that loses the fleet's thread to a runner that won it first.
//!
//! Fleet creation writes the thread, so the other runner's uncommitted assignment
//! holds that row's lock. This runner's assignment skips the locked thread rather
//! than wait on it, and takes nothing: no turn is read, so nothing is orphaned,
//! and the poll moves on as if the fleet were never assigned. The other
//! runner's assignment is held open in a transaction so the lock is certain rather
//! than a race the scheduler may or may not produce.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use af_core::id::Uuid7;
use af_core::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER};
use afd_fleet::run::sql::shared::ASSIGN_THREAD;
use sqlx::Connection as _;

use crate::seed::{Seeded, seeded};
use crate::support::Fixtures;

/// How long an assignment that skips the locked thread may take.
const SKIPPED_WITHIN: Duration = Duration::from_secs(10);

/// Which runner the thread names now.
const HOLDER: &str = "SELECT last_runner_id FROM thread.threads WHERE fleet_id = $1";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_assignment_behind_a_live_assignment_skips_and_takes_nothing() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, second],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let fleet_id = fleet.clone();
    let mut other = fixtures.database.acquire().await.expect("a connection");
    let mut assigning = other.begin().await.expect("a transaction");
    sqlx::query(ASSIGN_THREAD)
        .bind(&fleet)
        .bind(None::<Uuid7>)
        .bind(&second)
        .bind(RUN_TTL)
        .bind(RUNNER_OFFLINE_AFTER)
        .bind(afd_state::sql::RUNNER_STATUS_ACTIVE)
        .fetch_optional(&mut *assigning)
        .await
        .expect("the other runner's assignment wins, uncommitted");

    let polled = tokio::time::timeout(SKIPPED_WITHIN, fixtures.runs().select(&first, &[fleet_id]))
        .await
        .expect("the assignment skips the locked thread instead of waiting on it")
        .expect("a lost assignment is not a fault");
    assigning
        .commit()
        .await
        .expect("the other runner's assignment commits");
    drop(other);

    assert!(
        polled.is_none(),
        "the lost assignment takes nothing: {polled:?}"
    );
    let holder: Uuid7 = sqlx::query_scalar(HOLDER)
        .bind(&fleet)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the thread reads back");
    assert_eq!(holder, second, "the thread is the first contender's");
    fixtures.cleanup().await;
}
