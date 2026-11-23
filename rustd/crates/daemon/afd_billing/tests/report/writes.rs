//! What one write of usage lines charges: the rollup rows and the wallet move
//! in the inserting statement, once per line, under each line's own UTC day,
//! and concurrent writes on shared rows finish without a deadlock.

use super::fixture::{self, Fixture, Line};
use af_wire::usage::LineKind;
use afd_billing::report::dimension;
use afd_db::DbRole;
use afd_db::test_util::TestDatabase;
use std::sync::Arc;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

/// Writers released together on the same rollup rows.
const WRITERS: usize = 100;
/// Connections they share: enough that several write at once.
const POOL: &str = "16";

/// The statement-level triggers on `billing.usage_lines`, in the order
/// Postgres fires them: by name.
const STATEMENT_TRIGGERS: &str = "\
SELECT tgname::text FROM pg_trigger
 WHERE tgrelid = 'billing.usage_lines'::regclass AND NOT tgisinternal AND (tgtype & 1) = 0
 ORDER BY tgname";

/// A run across UTC midnight, written a day at a time and then again whole,
/// through the API login the product writes as: each day's lines land in that
/// day's rollup row, the run counts once on its first runtime slice's day, the
/// repeat adds nothing, and the report's days sum to the lines.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_report_sums_lines_by_day() {
    let f = Fixture::shared().await;
    let fleet = fixture::id();
    let mut first = Line::runtime(&f.tenant, &fleet, 17);
    first.at = "2026-10-09T23:59:59.900Z";
    let mut call = first.next(LineKind::Model, 0, 5);
    call.at = "2026-10-09T23:59:59.950Z";
    let mut slice = first.next(LineKind::Runtime, 60_000, 19);
    slice.at = "2026-10-10T00:00:00Z";
    let mut later = first.next(LineKind::Model, 1, 7);
    later.at = "2026-10-10T00:00:01Z";
    let whole = [first.clone(), call.clone(), slice.clone(), later.clone()];
    fixture::seed(&f.api, &[first, call]).await;
    fixture::seed(&f.api, &[slice, later]).await;
    fixture::seed(&f.api, &whole).await;

    let days: Vec<_> = fixture::usage_daily(&f.admin, &f.tenant)
        .await
        .into_iter()
        .map(|row| (row.day, row.charged_nanos, row.line_count))
        .collect();
    assert_eq!(
        days,
        [
            ("2026-10-09".to_owned(), 22, 2),
            ("2026-10-10".to_owned(), 26, 2)
        ]
    );
    assert_eq!(
        fixture::usage_daily(&f.admin, &f.tenant).await,
        fixture::ledger_daily(&f.admin, &f.tenant).await,
        "the rollup is the lines, summed by day"
    );
    let runs: Vec<_> = fixture::runs_daily(&f.admin, &f.tenant)
        .await
        .into_iter()
        .map(|row| (row.day, row.runs))
        .collect();
    assert_eq!(runs, [("2026-10-09".to_owned(), 1)]);
    assert_eq!(
        fixture::balance(&f.admin, &f.tenant).await,
        fixture::BALANCE - 48
    );

    let mut request = fixture::request();
    request.filter.fleet = Some(fleet);
    let report = f.billing.report(&f.tenant, &request).await.unwrap();
    assert_eq!(fixture::summary(&report).activity_count, Some(1));
    assert_eq!(fixture::summary(&report).charged_nanos, 48);
    let mut by_day: Vec<_> = report
        .aggregates
        .iter()
        .filter(|row| row.dimension == dimension::DAY && row.charged_nanos > 0)
        .map(|row| (row.day.unwrap().to_string(), row.charged_nanos))
        .collect();
    by_day.sort();
    assert_eq!(
        by_day,
        [("2026-10-09".to_owned(), 22), ("2026-10-10".to_owned(), 26)]
    );
    f.close().await;
}

/// A line created before a month ends counts in that month, whenever the
/// report is read.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_line_counts_in_the_month_it_was_written() {
    let f = Fixture::shared().await;
    let mut first = Line::runtime(&f.tenant, &f.fleet, 17);
    first.at = "2026-10-31T23:59:59.999Z";
    let mut call = first.next(LineKind::Model, 0, 5);
    call.at = "2026-11-01T00:00:00Z";
    fixture::seed(&f.admin, &[first, call]).await;
    let mut request = fixture::request();
    request.window.through = fixture::instant("2026-10-31T23:59:59.999Z");
    let october = f.billing.report(&f.tenant, &request).await.unwrap();
    let total = fixture::summary(&october);
    assert_eq!(
        (total.charged_nanos, total.line_count, total.activity_count),
        (17, 1, Some(1))
    );
    request.window.from = fixture::instant("2026-11-01T00:00:00Z");
    request.window.through = fixture::instant("2026-11-30T23:59:59.999Z");
    let november = f.billing.report(&f.tenant, &request).await.unwrap();
    let total = fixture::summary(&november);
    assert_eq!(
        (total.charged_nanos, total.line_count, total.activity_count),
        (5, 1, Some(0))
    );
    f.close().await;
}

/// A payer erased after its work ran still joins the rollup: the rollup keeps
/// opaque identifiers, as the ledger does, so a missing tenant or wallet
/// cannot refuse the write that logs the line.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_erased_payer_joins_the_rollup() {
    let f = Fixture::shared().await;
    let erased = fixture::id();
    fixture::seed(&f.admin, &[Line::runtime(&erased, &fixture::id(), 13)]).await;
    let usage = fixture::usage_daily(&f.admin, &erased).await;
    assert_eq!(
        usage
            .iter()
            .map(|row| (row.charged_nanos, row.line_count))
            .collect::<Vec<_>>(),
        [(13, 1)]
    );
    let runs = fixture::runs_daily(&f.admin, &erased).await;
    assert_eq!(runs.iter().map(|row| row.runs).sum::<i64>(), 1);
    f.close().await;
}

/// The wallet is the last row a write locks: Postgres fires a statement's
/// triggers in name order, and the debit's name must sort after the rollups'.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_wallet_debit_fires_after_the_rollups() {
    let f = Fixture::shared().await;
    let fired: Vec<String> = sqlx::query_scalar(STATEMENT_TRIGGERS)
        .fetch_all(&mut *f.admin.acquire().await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        fired,
        [
            "trg_usage_lines_add_daily",
            "trg_usage_lines_count_runs",
            "trg_usage_lines_debit_wallets"
        ]
    );
    f.close().await;
}

/// Every run has a slice on each of two days, on one fleet and model, so every
/// write updates the same two rollup rows and the one wallet; half list the
/// days in the other order. The triggers write rows in key order, so every
/// write finishes, and each line is charged once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn opposite_order_writes_on_shared_rows_finish_without_deadlock() {
    let f = Fixture::shared().await;
    let api = TestDatabase::shared()
        .open(DbRole::Api, &[("DATABASE_POOL_SIZE_API", POOL)])
        .await;
    let barrier = Arc::new(Barrier::new(WRITERS));
    let mut writing = JoinSet::new();
    for run in 0..WRITERS {
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
        let (api, barrier) = (api.clone(), Arc::clone(&barrier));
        writing.spawn(async move {
            barrier.wait().await;
            // A deadlock aborts one side with SQLSTATE 40P01, which fails here.
            fixture::seed(&api, &[first, slice]).await;
        });
    }
    while let Some(joined) = writing.join_next().await {
        joined.unwrap();
    }
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
    let runs: Vec<_> = written
        .runs
        .iter()
        .map(|row| (row.day.as_str(), row.runs))
        .collect();
    assert_eq!(runs, [("2026-10-20", 50), ("2026-10-21", 50)]);
    assert_eq!(written.balance, fixture::BALANCE - 110_000);
    assert_eq!(written.ledger, (200, 110_000));
    api.close().await;
    f.close().await;
}
