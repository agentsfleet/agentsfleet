//! Settlements racing on one run, one rollup row or one wallet: each line is
//! charged once, each run counted once, and none deadlocks another.

use super::fixture::{self, Fixture, Line};
use af_wire::usage::LineKind;
use afd_billing::biller::Biller as _;
use afd_billing::ledger::LedgerBiller;
use afd_db::test_util::TestDatabase;
use afd_db::{Db, DbRole};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

/// Callers released together for an exactly-once claim.
const CALLERS: usize = 100;
/// Connections they share: enough that several settle at once.
const POOL: &str = "16";
/// Below the API login's two-second lock bound, which the blocked debits wait under.
const BLOCKED_DEADLINE: Duration = Duration::from_millis(1_500);
const OBSERVATION_INTERVAL: Duration = Duration::from_millis(5);

/// Sessions waiting at the wallet debit behind `pid`'s locks, directly or
/// queued behind another waiter.
const BLOCKED_BEHIND: &str = "\
WITH RECURSIVE blocked(pid) AS (
    SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))
    UNION
    SELECT a.pid FROM pg_stat_activity a JOIN blocked b ON b.pid = ANY(pg_blocking_pids(a.pid))
)
SELECT COUNT(*)::bigint FROM pg_stat_activity a JOIN blocked b ON b.pid = a.pid
 WHERE a.query LIKE 'UPDATE billing.wallets%'";

/// A biller over an API pool wide enough for the callers to overlap.
async fn wide_biller() -> (Db, LedgerBiller) {
    let api = TestDatabase::shared()
        .open(DbRole::Api, &[("DATABASE_POOL_SIZE_API", POOL)])
        .await;
    (api.clone(), LedgerBiller::on_system_clock(api))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn concurrent_duplicate_settlements_charge_and_count_a_run_once() {
    let f = Fixture::shared().await;
    let first = Line::runtime(&f.tenant, &f.fleet, 17);
    fixture::seed(
        &f.admin,
        &[first.clone(), first.next(LineKind::Model, 0, 5)],
    )
    .await;
    let settlement = Arc::new(fixture::pending(&f.admin, &first.work()).await.remove(0));
    let (api, biller) = wide_biller().await;
    let barrier = Arc::new(Barrier::new(CALLERS));
    let (inside, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut callers = JoinSet::new();
    for _ in 0..CALLERS {
        let (biller, settlement, barrier) = (
            biller.clone(),
            Arc::clone(&settlement),
            Arc::clone(&barrier),
        );
        let (inside, peak) = (Arc::clone(&inside), Arc::clone(&peak));
        callers.spawn(async move {
            barrier.wait().await;
            let now = inside.fetch_add(1, Ordering::AcqRel) + 1;
            peak.fetch_max(now, Ordering::AcqRel);
            let receipt = biller.settle(&settlement).await;
            inside.fetch_sub(1, Ordering::AcqRel);
            receipt
        });
    }
    let mut charged = Vec::new();
    while let Some(joined) = callers.join_next().await {
        let receipt = joined.unwrap().unwrap();
        if receipt.settled_lines > 0 {
            charged.push((receipt.settled_lines, receipt.charged.as_i64()));
        }
    }
    assert!(peak.load(Ordering::Acquire) > 1, "the callers overlapped");
    assert_eq!(charged, [(2, 22)], "exactly one caller settles the lines");
    let written = fixture::written(&f.admin, &f.tenant).await;
    let usage: Vec<_> = written
        .usage
        .iter()
        .map(|row| (row.charged_nanos, row.line_count))
        .collect();
    assert_eq!(usage, [(22, 2)]);
    assert_eq!(
        written.runs.iter().map(|row| row.runs).collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(written.balance, fixture::BALANCE - 22);
    assert_eq!(written.ledger, (2, 22, 2));
    api.close().await;
    f.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn interleaved_settlements_of_one_run_count_it_once() {
    let f = Fixture::shared().await;
    let first = Line::runtime(&f.tenant, &f.fleet, 17);
    let mut slice = first.next(LineKind::Runtime, 60_000, 19);
    // Another model: its own settlement, rollup row and lock.
    slice.model = fixture::MODEL_B;
    fixture::seed(
        &f.admin,
        &[first.clone(), first.next(LineKind::Model, 0, 5), slice],
    )
    .await;
    let settlements = fixture::pending(&f.admin, &first.work()).await;
    assert_eq!(settlements.len(), 2, "one settlement per model");
    // Hold the wallet so each settlement stamps, rolls up and counts its lines,
    // then waits at its debit while the other does the same.
    let mut holding = f.admin.acquire().await.unwrap();
    let mut holder = sqlx::Connection::begin(&mut *holding).await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *holder)
        .await
        .unwrap();
    sqlx::query("SELECT 1 FROM billing.wallets WHERE tenant_id = $1 FOR UPDATE")
        .bind(&f.tenant)
        .execute(&mut *holder)
        .await
        .unwrap();
    let mut settling = JoinSet::new();
    for settlement in settlements {
        let biller = f.biller.clone();
        settling.spawn(async move { biller.settle(&settlement).await });
    }
    tokio::time::timeout(BLOCKED_DEADLINE, blocked_behind(&f.admin, pid, 2))
        .await
        .expect("both settlements reach their debit uncommitted");
    assert_eq!(
        fixture::ledger(&f.admin, &f.tenant).await,
        (3, 41, 0),
        "neither settlement has committed"
    );
    holder.commit().await.unwrap();
    drop(holding);
    assert_eq!(charged(settling).await, [(1, 19), (2, 22)]);
    let written = fixture::written(&f.admin, &f.tenant).await;
    assert_eq!(runs_by_day(&written), [("2026-10-25", 1)]);
    assert_eq!(
        usage_by_model(&written),
        [(fixture::MODEL_A, 22, 2), (fixture::MODEL_B, 19, 1)]
    );
    assert_eq!(written.balance, fixture::BALANCE - 41);
    assert_eq!(written.ledger, (3, 41, 3));
    f.close().await;
}

/// Waits until `count` sessions wait on `pid`'s locks.
async fn blocked_behind(admin: &Db, pid: i32, count: i64) {
    loop {
        let blocked: i64 = sqlx::query_scalar(BLOCKED_BEHIND)
            .bind(pid)
            .fetch_one(&mut *admin.acquire().await.unwrap())
            .await
            .unwrap();
        if blocked == count {
            return;
        }
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn opposite_order_settlements_on_shared_rollup_rows_finish_without_deadlock() {
    let f = Fixture::shared().await;
    let (api, biller) = wide_biller().await;
    // Every run has a slice on each of two days, on one fleet and model, so
    // every settlement updates the same two rollup rows; half list them in
    // the other order.
    let mut works = Vec::new();
    let mut lines = Vec::new();
    for run in 0..CALLERS {
        let money = i64::try_from(run).unwrap();
        let (start, end) = if run % 2 == 0 {
            ("2026-10-21T12:00:00Z", "2026-10-20T12:00:00Z")
        } else {
            ("2026-10-20T12:00:00Z", "2026-10-21T12:00:00Z")
        };
        let mut first = Line::runtime(&f.tenant, &f.fleet, money + 1);
        first.at = start;
        let mut slice = first.next(LineKind::Runtime, 60_000, money + 1_000);
        slice.at = end;
        works.push(first.work());
        lines.extend([first, slice]);
    }
    fixture::seed(&f.admin, &lines).await;
    let mut settlements = Vec::new();
    for work in &works {
        settlements.extend(fixture::pending(&f.admin, work).await);
    }
    let (settled, peak) = settle_together(&biller, settlements).await;
    assert!(peak > 1, "the settlements overlapped");
    assert_eq!(settled, 2 * CALLERS);
    let written = fixture::written(&f.admin, &f.tenant).await;
    let usage: Vec<_> = written
        .usage
        .iter()
        .map(|row| (row.day.as_str(), row.charged_nanos, row.line_count))
        .collect();
    assert_eq!(
        usage,
        [("2026-10-20", 55_000, 100), ("2026-10-21", 55_000, 100)]
    );
    assert_eq!(
        runs_by_day(&written),
        [("2026-10-20", 50), ("2026-10-21", 50)]
    );
    assert_eq!(written.balance, fixture::BALANCE - 110_000);
    assert_eq!(written.ledger, (200, 110_000, 200));
    api.close().await;
    f.close().await;
}

/// Each settlement's lines and charge, in order.
async fn charged(
    mut settling: JoinSet<afd_billing::Result<afd_billing::biller::Receipt>>,
) -> Vec<(usize, i64)> {
    let mut charged = Vec::new();
    while let Some(joined) = settling.join_next().await {
        let receipt = joined.unwrap().unwrap();
        charged.push((receipt.settled_lines, receipt.charged.as_i64()));
    }
    charged.sort_unstable();
    charged
}

/// The usage rows in `written`, by model: charge and line count.
fn usage_by_model(written: &fixture::Written) -> Vec<(&str, i64, i64)> {
    written
        .usage
        .iter()
        .map(|row| (row.model.as_str(), row.charged_nanos, row.line_count))
        .collect()
}

/// The run counts in `written`, by day.
fn runs_by_day(written: &fixture::Written) -> Vec<(&str, i64)> {
    written
        .runs
        .iter()
        .map(|row| (row.day.as_str(), row.runs))
        .collect()
}

/// Releases every settlement at once and answers the lines they settled and
/// the most that were inside the biller together.
async fn settle_together(
    biller: &LedgerBiller,
    settlements: Vec<afd_billing::biller::Settlement>,
) -> (usize, usize) {
    let barrier = Arc::new(Barrier::new(settlements.len()));
    let (inside, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut settling = JoinSet::new();
    for settlement in settlements {
        let (biller, barrier) = (biller.clone(), Arc::clone(&barrier));
        let (inside, peak) = (Arc::clone(&inside), Arc::clone(&peak));
        settling.spawn(async move {
            barrier.wait().await;
            let now = inside.fetch_add(1, Ordering::AcqRel) + 1;
            peak.fetch_max(now, Ordering::AcqRel);
            let receipt = biller.settle(&settlement).await;
            inside.fetch_sub(1, Ordering::AcqRel);
            receipt
        });
    }
    let mut settled = 0;
    while let Some(joined) = settling.join_next().await {
        // A deadlock aborts one side with SQLSTATE 40P01, which fails here.
        settled += joined
            .unwrap()
            .expect("no settlement deadlocks")
            .settled_lines;
    }
    (settled, peak.load(Ordering::Acquire))
}
