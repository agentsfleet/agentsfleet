//! The lease insert reads its clock after the slot lock it queues behind, so
//! the wait is not billed as runtime.
//!
//! The first renewal bills the span from the lease's metering cursor to its own
//! clock. A cursor stamped before the slot lock would make every moment spent
//! queueing for that lock count as run time. The holder here only locks the slot
//! row and writes nothing, so Postgres does not re-check the insert's predicate
//! after the wait and hide the clock read this guards.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_fleet::lease::Billed;
use sqlx::Connection as _;

use crate::seed::{self, MODEL, POSTURE, PROVIDER, Seeded, seeded};
use crate::support::Fixtures;

/// How long the slot lock is held after the insert queues behind it.
const HELD_FOR: Duration = Duration::from_millis(400);

/// Takes the fleet's slot row lock and writes nothing.
const LOCK_SLOT: &str =
    "SELECT pg_backend_pid() FROM runner.fleet_slots WHERE fleet_id = $1 FOR UPDATE";

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// Whether the lease's metering cursor starts at or after `$2`, at the
/// precision the column stores.
const CURSOR_STARTS_AFTER: &str = "SELECT last_metered_at >= date_trunc('milliseconds', $2::timestamptz) \
FROM runner.leases WHERE id = $1";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_wait_for_the_slot_lock_is_not_billed_as_runtime() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, _second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    let held = seed::select_fleet(&leases, &first, &fleet)
        .await
        .expect("the fleet is leasable");

    let mut holder = fixtures.database.acquire().await.expect("a connection");
    let mut locked = holder.begin().await.expect("holder transaction");
    let pid: i32 = sqlx::query_scalar(LOCK_SLOT)
        .bind(&fleet)
        .fetch_one(&mut *locked)
        .await
        .expect("take the slot lock");

    let billed = Billed {
        tenant_id: &tenant,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    let release = async {
        let mut probe = fixtures.database.acquire().await.expect("a connection");
        loop {
            let queued: bool = sqlx::query_scalar(BLOCKING)
                .bind(pid)
                .fetch_one(&mut *probe)
                .await
                .expect("inspect the lock queue");
            if queued {
                break;
            }
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(HELD_FOR).await;
        let released_at: jiff_sqlx::Timestamp = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *probe)
            .await
            .expect("read the clock");
        locked.rollback().await.expect("the holder lets go");
        released_at
    };

    let (issued, released_at) = tokio::join!(
        leases.issue(&first, &held, billed, fixtures.enrolled_at()),
        release
    );
    let issued = issued
        .expect("the lease row is written")
        .expect("the claim is still held");

    let mut probe = fixtures.database.acquire().await.expect("a connection");
    let after_wait: bool = sqlx::query_scalar(CURSOR_STARTS_AFTER)
        .bind(&issued.lease_id)
        .bind(released_at)
        .fetch_one(&mut *probe)
        .await
        .expect("read the cursor");
    assert!(
        after_wait,
        "the metering cursor starts when the slot lock was won, not before the wait"
    );
    fixtures.cleanup().await;
}
