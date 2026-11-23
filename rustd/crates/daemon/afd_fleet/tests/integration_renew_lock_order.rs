//! The host's renewal statement takes the host row before any thread, as an
//! issue and the per-run renewal do, so the two never wait on each other in
//! a cycle.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use af_wire::report::RenewRequest;
use afd_db::config::DbRole;
use afd_fleet::run::renew::Renewals;
use sqlx::Connection as _;

use crate::report_seed::{Held, held};

/// What an issue holds while it places a run: the host row, exclusively.
const HOLD_HOST: &str = "SELECT pg_backend_pid() FROM runner.runners WHERE id = $1 FOR UPDATE";

/// The backend `$1` is blocking, if any.
const BLOCKED_BY: &str = "SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))";

/// A thread lock that refuses to wait: it fails while another holds the thread.
const TRY_THREAD: &str = "SELECT 1 FROM thread.threads WHERE fleet_id = $1 FOR UPDATE NOWAIT";

/// How long a wait may take before the test calls it a hang.
const WAIT_LIMIT: Duration = Duration::from_secs(10);

/// Waits until some backend is blocked behind `pid`.
async fn blocked_by(held: &Held, pid: i32) {
    let mut observer = held.fixtures.database.acquire().await.expect("observer");
    let wait = async {
        loop {
            let found: Option<i32> = sqlx::query_scalar(BLOCKED_BY)
                .bind(pid)
                .fetch_optional(&mut *observer)
                .await
                .expect("inspect the lock waits");
            if found.is_some() {
                return;
            }
            tokio::task::yield_now().await;
        }
    };
    tokio::time::timeout(WAIT_LIMIT, wait)
        .await
        .expect("the renewal reaches the held host row");
}

// Value: protects=a renewal frame waiting on a held host row holds no thread;
// fails_when=the statement locks its threads before the host row, and an issue
// holding the host and wanting a thread deadlocks against it; why_new=the host
// batch statement replaced the per-run path's host-first lock; seam=none;
// no_claim=the run rows, which the statement locks after its threads.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_waiting_renewal_holds_no_thread() {
    let held = held().await;
    let owner = held.fixtures.open(DbRole::Migrator).await;
    let mut host_connection = owner.acquire().await.expect("host holder");
    let mut host = host_connection.begin().await.expect("open the host hold");
    let host_pid: i32 = sqlx::query_scalar(HOLD_HOST)
        .bind(&held.runner)
        .fetch_one(&mut *host)
        .await
        .expect("hold the host row");
    // Its own pool: the host hold keeps the first pool's connection.
    let prober = held.fixtures.open(DbRole::Migrator).await;
    let plane = held.fixtures.plane();

    let renewing = plane.renew_batch(Renewals {
        runner_id: &held.runner,
        runs: vec![(held.issued.run_id.clone(), RenewRequest::default())],
        funds_due: false,
        now: held.now,
        kept: std::collections::HashMap::new(),
    });
    let probing = async {
        blocked_by(&held, host_pid).await;
        let mut thread_connection = prober.acquire().await.expect("thread prober");
        let mut thread = thread_connection
            .begin()
            .await
            .expect("open the thread probe");
        let free = sqlx::query(TRY_THREAD)
            .bind(&held.fleet)
            .execute(&mut *thread)
            .await;
        thread.rollback().await.expect("release the thread probe");
        host.rollback().await.expect("release the host row");
        free
    };
    let (answers, free) = tokio::join!(renewing, probing);

    assert!(
        free.is_ok(),
        "the waiting renewal holds the thread: {free:?}"
    );
    assert!(
        answers.iter().all(Result::is_ok),
        "the renewal extends once the host is free: {answers:?}"
    );
    // A pool closes only once its connections are back.
    drop(host_connection);
    prober.close().await;
    owner.close().await;
    held.fixtures.cleanup().await;
}
