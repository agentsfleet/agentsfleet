//! What one settlement writes to the rollup: added once with the money, undone
//! with it, and filed under each line's own UTC day.

use super::cut_proxy::CutProxy;
use super::failures::{api_target, config_through};
use super::fixture::{self, DailyRow, Fixture, Line, RunRow, Written};
use af_core::error_code;
use af_wire::usage::LineKind;
use afd_billing::biller::{Biller as _, Receipt, Settlement};
use afd_billing::ledger::LedgerBiller;
use afd_billing::price::RatedLine;
use afd_billing::report::dimension;
use afd_billing::sql::work;
use afd_db::Db;
use afd_db::test_util::TestDatabase;
use afd_db::test_util::refusal::Refusal;

/// Each statement of the settle transaction, in order, after the line log.
/// A cut before any of them, or before the commit, must undo every one.
const SETTLE_STATEMENTS: [&[u8]; 5] = [
    b"UPDATE billing.usage_lines",
    b"INSERT INTO billing.usage_daily",
    b"INSERT INTO billing.runs_daily",
    b"UPDATE billing.wallets",
    b"COMMIT",
];

/// One fleet run of two lines and one setup attempt, logged and unsettled.
async fn run_and_setup(f: &Fixture) -> (Line, Line) {
    let first = Line::runtime(&f.tenant, &f.fleet, 17);
    let call = first.next(LineKind::Model, 0, 5);
    let setup = Line::setup(&f.tenant, 29);
    fixture::seed(&f.admin, &[first.clone(), call, setup.clone()]).await;
    (first, setup)
}

/// What settling [`run_and_setup`] in full writes.
fn settled(f: &Fixture) -> Written {
    let day = "2026-10-25".to_owned();
    let row = |work_type: &str, fleet_id, model: &str, charged_nanos, line_count| DailyRow {
        day: day.clone(),
        work_type: work_type.to_owned(),
        fleet_id,
        model: model.to_owned(),
        charged_nanos,
        input_tokens: line_count * fixture::INPUT,
        output_tokens: line_count * fixture::OUTPUT,
        runtime_ms: line_count * fixture::RUNTIME,
        line_count,
        unpriced_tokens: 0,
    };
    Written {
        usage: vec![
            row(work::AUTHORING_ATTEMPT, None, fixture::MODEL_B, 29, 1),
            row(
                work::FLEET_LEASE,
                Some(f.fleet.clone()),
                fixture::MODEL_A,
                22,
                2,
            ),
        ],
        runs: vec![RunRow {
            day,
            work_type: work::FLEET_LEASE.to_owned(),
            fleet_id: Some(f.fleet.clone()),
            runs: 1,
        }],
        balance: fixture::BALANCE - 51,
        ledger: (3, 51, 3),
    }
}

/// Nothing settled: the lines logged, the wallet whole, no rollup row.
fn unsettled() -> Written {
    Written {
        usage: Vec::new(),
        runs: Vec::new(),
        balance: fixture::BALANCE,
        ledger: (3, 51, 0),
    }
}

async fn settlements(f: &Fixture, first: &Line, setup: &Line) -> Vec<Settlement> {
    let mut all = fixture::pending(&f.admin, &first.work()).await;
    all.extend(fixture::pending(&f.admin, &setup.work()).await);
    assert_eq!(all.len(), 2, "one settlement per work: {all:?}");
    all
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn a_repeated_settle_pass_adds_nothing_to_the_rollup() {
    let f = Fixture::shared().await;
    let (first, setup) = run_and_setup(&f).await;
    let passes = settlements(&f, &first, &setup).await;
    let mut charged = Vec::new();
    for settlement in &passes {
        let receipt = f.biller.settle(settlement).await.unwrap();
        charged.push((receipt.settled_lines, receipt.charged.as_i64()));
    }
    assert_eq!(charged, [(2, 22), (1, 29)]);
    let expected = settled(&f);
    assert_eq!(fixture::written(&f.admin, &f.tenant).await, expected);
    for settlement in &passes {
        assert_eq!(
            f.biller.settle(settlement).await.unwrap(),
            Receipt::default(),
            "a second pass settles nothing"
        );
    }
    assert_eq!(
        fixture::settle(&f.biller, &f.admin, &first.work()).await,
        (0, 0)
    );
    assert_eq!(fixture::written(&f.admin, &f.tenant).await, expected);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn a_settle_cut_before_each_statement_rolls_back_the_rollup_and_wallet() {
    let f = Fixture::shared().await;
    let (first, setup) = run_and_setup(&f).await;
    let run = fixture::pending(&f.admin, &first.work()).await.remove(0);
    let (options, target) = api_target().await;
    for (ordinal, trigger) in SETTLE_STATEMENTS.iter().enumerate() {
        let proxy = CutProxy::new(target, trigger).await;
        let db = Db::connect(&config_through(&options, proxy.addr()))
            .await
            .unwrap();
        let error = LedgerBiller::on_system_clock(db.clone())
            .settle(&run)
            .await
            .unwrap_err();
        assert!(
            error.is_datastore_unavailable(),
            "boundary {ordinal}: {error}"
        );
        assert!(!error.is_contended(), "boundary {ordinal}: {error}");
        assert_eq!(proxy.cuts(), 1, "boundary {ordinal} fires once");
        assert_eq!(
            fixture::written(&f.admin, &f.tenant).await,
            unsettled(),
            "boundary {ordinal} leaves nothing behind"
        );
        db.close().await;
        proxy.close().await;
    }
    let retry = f.biller.settle(&run).await.unwrap();
    assert_eq!((retry.settled_lines, retry.charged.as_i64()), (2, 22));
    fixture::settle(&f.biller, &f.admin, &setup.work()).await;
    assert_eq!(fixture::written(&f.admin, &f.tenant).await, settled(&f));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn a_settle_refused_at_the_debit_rolls_back_the_rollup_and_wallet() {
    let f = Fixture::shared().await;
    let (first, setup) = run_and_setup(&f).await;
    let run = fixture::pending(&f.admin, &first.work()).await.remove(0);
    let refusal = Refusal::install(
        &TestDatabase::shared().url(),
        "billing.wallets",
        "refuse_debit",
        f.tenant.as_str(),
        &format!("NEW.tenant_id = '{}'::uuid", f.tenant),
        "RAISE EXCEPTION 'the fixture refuses this debit'",
    )
    .await
    .expect("the fixture refusal installs");
    let error = f.biller.settle(&run).await.unwrap_err();
    // A statement that failed on its own terms is a defect, not an outage.
    assert!(!error.is_datastore_unavailable(), "{error}");
    assert!(!error.is_contended(), "{error}");
    assert_eq!(error.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(fixture::written(&f.admin, &f.tenant).await, unsettled());
    drop(refusal);
    let retry = f.biller.settle(&run).await.unwrap();
    assert_eq!((retry.settled_lines, retry.charged.as_i64()), (2, 22));
    fixture::settle(&f.biller, &f.admin, &setup.work()).await;
    assert_eq!(fixture::written(&f.admin, &f.tenant).await, settled(&f));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn a_run_across_utc_midnight_splits_money_by_day_and_counts_once_on_its_first_runtime_day() {
    let f = Fixture::shared().await;
    // The relay may settle either day's batch first; both orders end the same.
    for later_day_first in [false, true] {
        let fleet = fixture::id();
        let mut first = Line::runtime(&f.tenant, &fleet, 17);
        first.at = "2026-10-09T23:59:59.900Z";
        let mut call = first.next(LineKind::Model, 0, 5);
        call.at = "2026-10-09T23:59:59.950Z";
        let mut slice = first.next(LineKind::Runtime, 60_000, 19);
        slice.at = "2026-10-10T00:00:00Z";
        let mut later = first.next(LineKind::Model, 1, 7);
        later.at = "2026-10-10T00:00:01Z";
        fixture::seed(&f.admin, &[first.clone(), call.clone(), slice, later]).await;
        let day_one = |rated: &RatedLine| fixture::is(rated, &first) || fixture::is(rated, &call);
        let work = first.work();
        let batch = if later_day_first {
            fixture::settle_only(&f.biller, &f.admin, &work, |rated| !day_one(rated)).await
        } else {
            fixture::settle_only(&f.biller, &f.admin, &work, day_one).await
        };
        let counted = runs_of(&f, &fleet).await;
        if later_day_first {
            assert_eq!(batch, (2, 26));
            assert_eq!(counted, [], "a later slice never counts the run");
        } else {
            assert_eq!(batch, (2, 22));
            assert_eq!(counted, [("2026-10-09".to_owned(), 1)]);
        }
        let rest = fixture::settle(&f.biller, &f.admin, &work).await;
        assert_eq!(rest.0, 2);
        assert_eq!(runs_of(&f, &fleet).await, [("2026-10-09".to_owned(), 1)]);
        let days: Vec<_> = fixture::usage_daily(&f.admin, &f.tenant)
            .await
            .into_iter()
            .filter(|row| row.fleet_id.as_ref() == Some(&fleet))
            .map(|row| (row.day, row.charged_nanos, row.line_count))
            .collect();
        assert_eq!(
            days,
            [
                ("2026-10-09".to_owned(), 22, 2),
                ("2026-10-10".to_owned(), 26, 2)
            ]
        );
        let mut request = fixture::request();
        request.filter.fleet = Some(fleet.clone());
        let report = f.billing.report(&f.tenant, &request).await.unwrap();
        assert_eq!(fixture::summary(&report).activity_count, Some(1));
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
    }
    assert_eq!(
        fixture::balance(&f.admin, &f.tenant).await,
        fixture::BALANCE - 2 * 48
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn a_line_created_before_month_end_and_settled_after_counts_in_its_month() {
    let f = Fixture::shared().await;
    let mut first = Line::runtime(&f.tenant, &f.fleet, 17);
    first.at = "2026-10-31T23:59:59.999Z";
    let mut call = first.next(LineKind::Model, 0, 5);
    call.at = "2026-11-01T00:00:00Z";
    fixture::record(&f, &[first, call]).await;
    let settled_after: bool = sqlx::query_scalar(
        "SELECT bool_and(settled_at >= '2026-11-01T00:00:00Z') FROM billing.usage_lines WHERE tenant_id = $1",
    )
    .bind(&f.tenant)
    .fetch_one(&mut *f.admin.acquire().await.unwrap())
    .await
    .unwrap();
    assert!(settled_after, "both lines settle after October ends");
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
    assert_eq!(runs_of(&f, &f.fleet).await, [("2026-10-31".to_owned(), 1)]);
    f.close().await;
}

/// The run count rows of `fleet`, by day.
async fn runs_of(f: &Fixture, fleet: &af_core::id::Uuid7) -> Vec<(String, i64)> {
    fixture::runs_daily(&f.admin, &f.tenant)
        .await
        .into_iter()
        .filter(|row| row.fleet_id.as_ref() == Some(fleet))
        .map(|row| (row.day, row.runs))
        .collect()
}
