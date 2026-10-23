//! The billing sweeper when its biller misses, and when a report beats it to
//! a lease.
//!
//! `integration_billing_sweep.rs` proves a pass finishes what the lease path
//! left. These prove a pass that cannot finish it counts the miss and leaves
//! the work for the next pass, and that a lease another writer billed while
//! the pass waited for it is skipped rather than billed twice.
//!
//! Each case runs in its own database: a pass sweeps every row it can see.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afd_billing::ledger::LedgerBiller;
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::billing_sweep::{BillingSwept, sweep};
use afd_fleet::lease::sql::LEASE_STATUS_REPORTED;
use sqlx::Connection as _;

use crate::faulty_biller::{Faults, Faulty};
use crate::integration_billing_sweep::{abandon, orphan};
use crate::report_seed::{Held, held_in};
use crate::support::Fixtures;

/// How long the race case waits for the pass to queue behind the lock.
const QUEUED_WITHIN: Duration = Duration::from_secs(10);

/// How often it looks.
const POLL: Duration = Duration::from_millis(20);

/// Locks the lease row the pass is about to bill, answering the holder's pid.
const HOLD_LEASE: &str =
    "SELECT pg_backend_pid() FROM runner.leases WHERE id = $1::uuid FOR UPDATE";

/// Sessions queued behind `$1`.
const WAITERS: &str = "SELECT count(*) FROM pg_stat_activity \
     WHERE $1::integer = ANY(pg_blocking_pids(pid))";

/// What a late report does to the cursor: carries it to the deadline.
const BILL_TO_DEADLINE: &str =
    "UPDATE runner.leases SET last_metered_at = lease_expires_at WHERE id = $1::uuid";

async fn isolated() -> Held {
    held_in(Fixtures::create_isolated_with_queue().await).await
}

async fn close(held: Held) {
    held.fixtures.cleanup().await;
}

/// A reported lease's open hold the biller cannot release is counted and
/// left open; the next pass, with the biller back, releases it as completed.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_finished_hold_the_biller_cannot_release_waits_for_the_next_pass() {
    let held = isolated().await;
    let database = &held.fixtures.database;
    let ledger = LedgerBiller::on_system_clock(database.clone());
    orphan(&held, &ledger).await;
    sqlx::query("UPDATE runner.leases SET status = $2 WHERE id = $1::uuid")
        .bind(held.lease_key.as_str())
        .bind(LEASE_STATUS_REPORTED)
        .execute(&mut *database.acquire().await.expect("a connection"))
        .await
        .expect("the lease reported before its release was lost");
    let down = Faulty::over(
        database,
        Faults {
            settle: true,
            release: true,
            ..Faults::NONE
        },
    );

    let missed = sweep(database, down.as_ref(), &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    let recovered = sweep(database, &ledger, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");

    assert_eq!(
        missed,
        BillingSwept {
            settle_failures: 1,
            release_failures: 1,
            ..BillingSwept::default()
        }
    );
    assert_eq!(
        recovered,
        BillingSwept {
            resettled: 1,
            released: 1,
            ..BillingSwept::default()
        }
    );
    close(held).await;
}

/// An abandoned lease's last slice is logged even when the biller cannot
/// settle it; the pass counts the miss and the line sweep settles it later.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_expired_slice_the_biller_cannot_settle_is_logged_and_counted() {
    let held = isolated().await;
    let database = &held.fixtures.database;
    abandon(&held).await;
    let down = Faulty::over(
        database,
        Faults {
            settle: true,
            ..Faults::NONE
        },
    );

    let swept = sweep(database, down.as_ref(), &Entropy::new(), held.now)
        .await
        .expect("the pass reads");

    assert_eq!(
        swept,
        BillingSwept {
            expired_settle_failures: 1,
            ..BillingSwept::default()
        }
    );
    let logged: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing.usage_lines WHERE work_id = $1::uuid")
            .bind(held.lease_key.as_str())
            .fetch_one(&mut *database.acquire().await.expect("a connection"))
            .await
            .expect("the lines count");
    assert_eq!(logged, 1, "the slice is owed, logged for the line sweep");
    close(held).await;
}

/// A lease a late report billed while the pass waited on its row is skipped:
/// the pass re-reads the row under the lock, finds the cursor at the
/// deadline, and bills nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_lease_billed_while_the_pass_waited_is_skipped() {
    let held = isolated().await;
    let database = held.fixtures.database.clone();
    abandon(&held).await;
    let lease = held.lease_key.as_str().to_owned();
    let mut reporter = database.acquire().await.expect("a connection");
    let mut report = reporter.begin().await.expect("a transaction");
    let pid: i32 = sqlx::query_scalar(HOLD_LEASE)
        .bind(&lease)
        .fetch_one(&mut *report)
        .await
        .expect("the lease row locks");

    let biller = Arc::new(LedgerBiller::on_system_clock(database.clone()));
    let pass = tokio::spawn({
        let (database, biller, now) = (database.clone(), Arc::clone(&biller), held.now);
        async move { sweep(&database, biller.as_ref(), &Entropy::new(), now).await }
    });
    let deadline = tokio::time::Instant::now() + QUEUED_WITHIN;
    let mut queued = 0_i64;
    while queued == 0 && tokio::time::Instant::now() < deadline {
        queued = sqlx::query_scalar(WAITERS)
            .bind(pid)
            .fetch_one(&mut *database.acquire().await.expect("a connection"))
            .await
            .expect("the waiters count");
        tokio::time::sleep(POLL).await;
    }
    sqlx::query(BILL_TO_DEADLINE)
        .bind(&lease)
        .execute(&mut *report)
        .await
        .expect("the late report carries the cursor");
    report.commit().await.expect("the late report commits");
    drop(reporter);
    let swept = pass.await.expect("the pass joins").expect("the pass reads");

    assert_eq!(queued, 1, "the pass queued behind the late report");
    assert_eq!(swept, BillingSwept::default(), "nothing is billed twice");
    close(held).await;
}
