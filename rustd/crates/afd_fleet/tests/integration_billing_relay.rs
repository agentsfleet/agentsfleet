//! The lease path logs usage and returns; the billing relay is the only thing
//! that moves the money.
//!
//! Each test takes a private database, because a relay pass settles every
//! unsettled line it can see and its counts are exact only where the test's
//! rows are the only ones.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_billing::ledger::test_util::LOCK_TIMEOUT;
use afd_billing::ledger::{LedgerBiller, log_lines};
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::billing_sweep::sweep;
use afd_wire::report::RenewRequest;
use afd_wire::usage::{RequestLines, RequestUsage};
use sqlx::Connection as _;

use crate::integration_billing_sweep::settlement;
use crate::integration_held_sandbox::report::processed;
use crate::report_seed::{DEEP_POOL, Held, held_in};
use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

/// `(lines, unsettled lines, charged nanos)` logged for one lease.
const LINES: &str = "SELECT count(*), count(*) FILTER (WHERE settled_at IS NULL), \
     COALESCE(sum(charged_nanos), 0)::BIGINT FROM billing.usage_lines WHERE work_id = $1::uuid";

async fn isolated() -> Held {
    held_in(Fixtures::create_isolated_with_queue().await).await
}

fn biller(held: &Held) -> LedgerBiller {
    LedgerBiller::new(
        held.fixtures.database.clone(),
        Entropy::new(),
        Arc::new(afd_core::clock::SystemClock),
    )
}

/// `held`'s lease's lines, unsettled lines and total charge.
async fn lines(held: &Held) -> (i64, i64, i64) {
    sqlx::query_as(LINES)
        .bind(held.lease_key.as_str())
        .fetch_one(
            &mut *held
                .fixtures
                .database
                .acquire()
                .await
                .expect("a pooled connection"),
        )
        .await
        .expect("the lines read")
}

/// A report commits and returns while another
/// transaction holds its tenant's wallet row, because it moves no money. Once
/// the row is free, one relay pass charges exactly what the report logged.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn report_returns_while_the_wallet_row_is_locked() {
    let held = isolated().await;
    let database = &held.fixtures.database;
    let plane = held.fixtures.plane();
    let report = processed(&held, None);

    let mut locker = database.acquire().await.expect("a pooled connection");
    let mut lock = locker.begin().await.expect("a transaction opens");
    sqlx::query("SELECT 1 FROM billing.wallets WHERE tenant_id = $1::uuid FOR UPDATE")
        .bind(&held.tenant)
        .fetch_one(&mut *lock)
        .await
        .expect("the wallet row locks");

    tokio::time::timeout(LOCK_TIMEOUT, plane.report(&held.runner, &report, held.now))
        .await
        .expect("the report never waits on the wallet lock")
        .expect("the report settles the lease");
    let (logged, unsettled, charged) = lines(&held).await;
    assert!(logged > 0, "the report logs its runtime line");
    assert_eq!(unsettled, logged, "the report charges nothing itself");
    assert_eq!(held.fixtures.balance(&held.tenant).await, Some(DEEP_POOL));

    lock.rollback().await.expect("the lock is released");
    drop(locker);
    let swept = sweep(database, &biller(&held), &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(swept.resettled, 1, "{swept:?}");
    assert_eq!(lines(&held).await.1, 0, "every line is settled");
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - charged),
        "the relay charges what the report logged, once"
    );

    drop(plane);
    held.fixtures.cleanup().await;
}

/// A renewal logs its slice and its requests and settles none
/// of them; the balance moves only when the relay runs.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_logs_its_slice_and_leaves_it_unsettled() {
    let held = isolated().await;
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let plane = held.fixtures.plane();
    let requests = RequestLines(vec![RequestUsage {
        seq: 0,
        at: held.now,
        input_tokens: 1_000,
        cached_input_tokens: 0,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: 100,
    }]);

    plane
        .renew(
            &held.runner,
            held.issued.lease_id.as_str(),
            RenewRequest { requests },
            held.now,
        )
        .await
        .expect("a funded lease renews");

    let (logged, unsettled, charged) = lines(&held).await;
    assert!(logged > 0, "the renewal logs the request line");
    assert!(charged > 0, "the seeded rate prices it");
    assert_eq!(unsettled, logged, "the renewal charges nothing itself");
    assert_eq!(held.fixtures.balance(&held.tenant).await, Some(DEEP_POOL));

    drop(plane);
    held.fixtures.cleanup().await;
}

/// With no wait, one pass settles a reported lease's lines, and a second pass
/// finds nothing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn one_pass_settles_a_reported_lease() {
    let held = isolated().await;
    let database = &held.fixtures.database;
    let plane = held.fixtures.plane();
    plane
        .report(&held.runner, &processed(&held, None), held.now)
        .await
        .expect("the report settles the lease");

    let swept = sweep(database, &biller(&held), &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(swept.resettled, 1, "{swept:?}");
    let charged = lines(&held).await.2;
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - charged)
    );
    let again = sweep(database, &biller(&held), &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(again.resettled, 0, "{again:?}");

    drop(plane);
    held.fixtures.cleanup().await;
}

/// Many relays (two daemons, or a pass that overlapped the next) pass over
/// the same line at once, and it is charged exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_passes_charge_each_line_once() {
    const PASSES: usize = 100;
    let held = isolated().await;
    let database = &held.fixtures.database;
    let work = settlement(&held).await;
    let charged = work
        .lines
        .iter()
        .map(|line| line.charged.as_i64())
        .sum::<i64>();
    log_lines(
        &mut *database.acquire().await.expect("a pooled connection"),
        &Entropy::new(),
        held.now,
        &work,
    )
    .await
    .expect("the line is logged");

    let start = Arc::new(tokio::sync::Barrier::new(PASSES));
    let (in_flight, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let passes: Vec<_> = (0..PASSES)
        .map(|_| {
            let (database, start) = (database.clone(), Arc::clone(&start));
            let (in_flight, peak, now) = (Arc::clone(&in_flight), Arc::clone(&peak), held.now);
            tokio::spawn(async move {
                let biller = LedgerBiller::on_system_clock(database.clone());
                start.wait().await;
                peak.fetch_max(
                    in_flight.fetch_add(1, Ordering::SeqCst) + 1,
                    Ordering::SeqCst,
                );
                let swept = sweep(&database, &biller, &Entropy::new(), now).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
                swept.expect("the pass reads").resettled
            })
        })
        .collect();
    let mut settled = 0;
    for pass in passes {
        settled += pass.await.expect("the pass task finishes");
    }
    assert!(peak.load(Ordering::SeqCst) > 1, "the passes overlapped");
    assert!(settled >= 1, "at least one pass settles the work");
    assert_eq!(lines(&held).await.1, 0, "the line is settled");
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - charged),
        "the line is charged once among them all"
    );

    held.fixtures.cleanup().await;
}
