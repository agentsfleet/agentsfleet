//! M240_002: the lease path logs usage and returns; the billing relay is the
//! only thing that moves the money.
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
use std::time::Duration;

use afd_billing::biller::{Biller as _, Hold, Reservation};
use afd_billing::ledger::{LedgerBiller, log_lines};
use afd_billing::Nanos;
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::billing_sweep::{GRACE, sweep};
use afd_wire::report::RenewRequest;
use afd_wire::usage::{RequestLines, RequestUsage};
use sqlx::Connection as _;

use crate::integration_billing_sweep::settlement;
use crate::integration_held_sandbox::report::processed;
use crate::report_seed::{DEEP_POOL, Held, held_in};
use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

/// The hold a test places before the lease reports.
const HELD: Nanos = Nanos::from_i64(90_000);

/// The lock timeout a billing statement gives up at: a report that waited on
/// the wallet would take at least this long.
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

/// `(lines, unsettled lines, charged nanos)` logged for one lease.
const LINES: &str = "SELECT count(*), count(*) FILTER (WHERE settled_at IS NULL), \
     COALESCE(sum(charged_nanos), 0)::BIGINT FROM billing.usage_lines WHERE work_id = $1::uuid";

/// Open holds of one lease.
const OPEN_HOLDS: &str =
    "SELECT count(*) FROM billing.holds WHERE work_id = $1::uuid AND released_at IS NULL";

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
        .fetch_one(&mut *held.fixtures.database.acquire().await.expect("a pooled connection"))
        .await
        .expect("the lines read")
}

/// `held`'s lease's open holds.
async fn open_holds(held: &Held) -> i64 {
    sqlx::query_scalar(OPEN_HOLDS)
        .bind(held.lease_key.as_str())
        .fetch_one(&mut *held.fixtures.database.acquire().await.expect("a pooled connection"))
        .await
        .expect("the holds count")
}

/// Places the lease's hold, as issue does after it commits.
async fn hold(held: &Held, biller: &LedgerBiller) {
    let work = settlement(held).await;
    let placed = biller
        .reserve(&Reservation {
            work: work.work,
            payer: work.payer,
            ceiling: HELD,
        })
        .await
        .expect("the hold is placed");
    assert!(matches!(placed, Hold::Held { .. }), "{placed:?}");
}

/// Dimension 1.1, the Goal: a report commits and returns while another
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

/// Dimension 1.2: a renewal logs its slice and its requests and settles none
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

/// Dimension 2.1: with no wait, one pass settles a reported lease's lines and
/// releases its hold, and a second pass finds nothing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn one_pass_settles_a_reported_lease_and_releases_its_hold() {
    let held = isolated().await;
    let database = &held.fixtures.database;
    let biller = biller(&held);
    hold(&held, &biller).await;
    let plane = held.fixtures.plane();
    plane
        .report(&held.runner, &processed(&held, None), held.now)
        .await
        .expect("the report settles the lease");
    assert_eq!(open_holds(&held).await, 1, "the report leaves its hold to the relay");

    let swept = sweep(database, &biller, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!((swept.resettled, swept.released), (1, 1), "{swept:?}");
    assert_eq!(open_holds(&held).await, 0, "the hold is released");
    let charged = lines(&held).await.2;
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - charged)
    );
    let again = sweep(database, &biller, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!((again.resettled, again.released), (0, 0), "{again:?}");

    drop(plane);
    held.fixtures.cleanup().await;
}

/// Dimension 2.2: an expired lease that never reported keeps its hold through
/// [`GRACE`], so a late report still wins; past the grace the relay frees it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_expired_lease_keeps_its_hold_until_the_grace_passes() {
    let held = isolated().await;
    let database = &held.fixtures.database;
    let biller = biller(&held);
    hold(&held, &biller).await;
    let expire = |age: Duration| {
        let lease = held.lease_key.as_str().to_owned();
        async move {
            sqlx::query(
                "UPDATE runner.leases SET status = $2, updated_at = now() - $3::interval \
                 WHERE id = $1::uuid",
            )
            .bind(lease)
            .bind(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
            .bind(age)
            .execute(&mut *database.acquire().await.expect("a pooled connection"))
            .await
            .expect("the lease ends unreported");
        }
    };

    expire(Duration::ZERO).await;
    let swept = sweep(database, &biller, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(swept.released, 0, "{swept:?}");
    assert_eq!(open_holds(&held).await, 1, "a late report may still come");

    expire(GRACE.saturating_mul(2)).await;
    let swept = sweep(database, &biller, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(swept.released, 1, "{swept:?}");
    assert_eq!(open_holds(&held).await, 0);

    held.fixtures.cleanup().await;
}

/// Dimension 2.4: two daemons' relays pass over the same lines at once, and
/// each line is charged exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_concurrent_passes_charge_each_line_once() {
    let held = isolated().await;
    let database = &held.fixtures.database;
    let work = settlement(&held).await;
    let charged = work.lines.iter().map(|line| line.charged.as_i64()).sum::<i64>();
    log_lines(
        &mut *database.acquire().await.expect("a pooled connection"),
        &Entropy::new(),
        held.now,
        &work,
    )
    .await
    .expect("the line is logged");

    let (first, second) = (biller(&held), biller(&held));
    let (mint_one, mint_two) = (Entropy::new(), Entropy::new());
    let (one, two) = tokio::join!(
        sweep(database, &first, &mint_one, held.now),
        sweep(database, &second, &mint_two, held.now),
    );
    let settled = one.expect("the first pass reads").resettled
        + two.expect("the second pass reads").resettled;
    assert!(settled >= 1, "at least one pass settles the work");
    assert_eq!(lines(&held).await.1, 0, "the line is settled");
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - charged),
        "the line is charged once between the two"
    );

    held.fixtures.cleanup().await;
}
