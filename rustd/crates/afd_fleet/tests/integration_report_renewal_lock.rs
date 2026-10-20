//! A report that waits behind a renewal numbers its runtime line after the
//! renewal's.
//!
//! The metering statement counts the lease's runtime lines to number the next
//! one. Under READ COMMITTED a statement that waits on a row lock re-reads the
//! locked row but keeps its starting snapshot for everything else, so a report
//! whose statement began while a renewal was still open counted the lines
//! before the renewal's landed, took the same number, lost its line to the
//! `(work, seq)` guard, and advanced the cursor anyway: a runtime slice nobody
//! was billed for. Both paths now take the host's capacity lock first, so the
//! report's statement starts only after the renewal committed.
//!
//! The renewal is held mid-transaction — metered, line not yet inserted — by a
//! row lock on the fleet's activity counter, which the usage line's insert
//! trigger updates. Every wait is observed through `pg_stat_activity` alone:
//! polling a table another test may run DDL on while a lock is held is how
//! fe10062c4's hang happened.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_db::config::DbRole;
use afd_fleet::lease::sql::meter::Metering;
use afd_fleet::lease::{Committed, Renewed};
use sqlx::Connection as _;

use crate::queue;
use crate::report_commit::{RESPONSE_ACCEPTED, report};
use crate::report_seed::{Held, held};

/// The first line number the daemon's runtime lines take.
const BASE: i64 = afd_wire::usage::DAEMON_SEQ_BASE as i64;

/// Holds the row the renewal's usage-line insert trigger must update.
const HOLD_COUNTER: &str =
    "SELECT pg_backend_pid() FROM fleet.activity_counters WHERE fleet_id = $1::uuid FOR UPDATE";

/// The backend `$1` is blocking, if any.
const BLOCKED_BY: &str = "SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))";

/// The lease's runtime lines, by number, with the run time each billed.
const RUNTIME_LINES: &str = "SELECT seq, runtime_ms FROM billing.usage_lines \
     WHERE work_id = $1::uuid AND seq >= $2 ORDER BY seq";

/// How long a wait may take before the test calls it a hang.
const WAIT_LIMIT: Duration = Duration::from_secs(10);

/// Run time the report accrues while the renewal is held, so its slice is
/// never under the millisecond the meter rounds to.
const REPORT_SLICE: Duration = Duration::from_millis(50);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_behind_a_held_renewal_bills_its_own_runtime_line() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    let from = held.run_a_slice().await;
    let lease = held
        .leases
        .load_for_report(lease_id, &held.runner)
        .await
        .expect("the lease load must reach the datastore")
        .expect("the seeded lease belongs to the seeded runner");

    // The blocker is scaffolding, so it holds its row lock as the lane's
    // owner: `FOR UPDATE` asks for `UPDATE` on the counter, which `api_runtime`
    // reaches only through the usage-line trigger.
    let owner = held.fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("blocker");
    let mut blocker = connection.begin().await.expect("open the blocker");
    let blocker_pid: i32 = sqlx::query_scalar(HOLD_COUNTER)
        .bind(&held.fleet)
        .fetch_one(&mut *blocker)
        .await
        .expect("the fleet's activity counter exists once an event was received");

    let renew = held.leases.extend(
        Metering {
            lease_id,
            runner_id: &held.runner,
            requests: &[],
        },
        &held.lease_key,
        &held.pricing,
        held.now,
    );
    let sequenced = async {
        let renewal_pid = blocked_by(&held, blocker_pid).await;
        let report_pass = held.leases.commit_report(report(
            lease_id,
            &held.runner,
            &lease,
            (&held.lease_key, &held.pricing),
            RESPONSE_ACCEPTED,
            held.now,
        ));
        let release = async {
            blocked_by(&held, renewal_pid).await;
            tokio::time::sleep(REPORT_SLICE).await;
            blocker.rollback().await.expect("release the renewal");
        };
        let (committed, ()) = tokio::join!(report_pass, release);
        committed
    };
    let (renewed, committed) = tokio::join!(renew, sequenced);

    assert!(
        matches!(
            renewed,
            Ok(Renewed::Extended {
                settlement: Some(_),
                ..
            })
        ),
        "the renewal extends and bills its slice: {renewed:?}"
    );
    assert!(
        matches!(
            committed,
            Ok(Committed::Settled {
                settlement: Some(_),
                ..
            })
        ),
        "the report settles and bills its slice: {committed:?}"
    );
    let lines = runtime_lines(&held).await;
    let seqs: Vec<i64> = lines.iter().map(|(seq, _ms)| *seq).collect();
    assert_eq!(
        seqs,
        [BASE, BASE + 1],
        "each pass logs its own runtime line under its own number: {lines:?}"
    );
    let billed: i64 = lines.iter().map(|(_seq, ms)| ms).sum();
    assert_eq!(
        held.metered_from(from).await,
        billed,
        "every millisecond the cursor advanced over is on a line: {lines:?}"
    );

    drop(connection);
    owner.close().await;
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Waits until some backend is blocked by `pid`, and answers that backend.
async fn blocked_by(held: &Held, pid: i32) -> i32 {
    let mut observer = held.fixtures.database.acquire().await.expect("observer");
    let wait = async {
        loop {
            let found: Option<i32> = sqlx::query_scalar(BLOCKED_BY)
                .bind(pid)
                .fetch_optional(&mut *observer)
                .await
                .expect("inspect the lock waits");
            if let Some(waiter) = found {
                return waiter;
            }
            tokio::task::yield_now().await;
        }
    };
    tokio::time::timeout(WAIT_LIMIT, wait)
        .await
        .expect("the next pass reaches the held lock")
}

/// The lease's runtime lines as `(seq, runtime_ms)`.
async fn runtime_lines(held: &Held) -> Vec<(i64, i64)> {
    let mut connection = held.fixtures.database.acquire().await.expect("read lines");
    sqlx::query_as(RUNTIME_LINES)
        .bind(held.issued.lease_id.as_str())
        .bind(BASE)
        .fetch_all(&mut *connection)
        .await
        .expect("the lines read")
}
