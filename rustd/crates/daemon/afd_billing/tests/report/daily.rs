use super::fixture::{self, Fixture, Line};
use af_wire::usage::LineKind;
use afd_billing::report::{Aggregate, Filter, Report, dimension};
use afd_billing::sql::work;

const BEFORE_MIDNIGHT: &str = "2026-09-30T23:59:59.999Z";
const MIDNIGHT: &str = "2026-10-01T00:00:00Z";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn daily_spend_reconciles_filtered_snapshot() {
    let f = Fixture::shared().await;
    seed_days(&f).await;
    let mut request = fixture::request();
    request.window.from = fixture::instant(BEFORE_MIDNIGHT);
    for filter in [
        Filter::default(),
        Filter {
            fleet: Some(f.fleet.clone()),
            ..Filter::default()
        },
        Filter {
            work_type: Some(work::AUTHORING_ATTEMPT),
            ..Filter::default()
        },
        Filter {
            fleet: Some(f.fleet.clone()),
            work_type: Some(work::FLEET_RUN),
        },
    ] {
        request.filter = filter;
        let report = f.billing.report(&f.tenant, &request).await.unwrap();
        assert_additive_totals(&report);
    }
    request.filter = Filter::default();
    let report = f.billing.report(&f.tenant, &request).await.unwrap();
    // One run, counted once on its first runtime slice's day; setup is no run.
    assert_eq!(fixture::summary(&report).activity_count, Some(1));
    assert!(days(&report).all(|day| day.activity_count.is_none()));
    let mut dates: Vec<_> = days(&report)
        .map(|day| (day.day.unwrap().to_string(), day.charged_nanos))
        .collect();
    dates.sort();
    assert_eq!(
        dates,
        [("2026-09-30".to_owned(), 17), ("2026-10-01".to_owned(), 52)]
    );
    assert_eq!(fixture::summary(&report).charged_nanos, 69);
    assert_eq!(fixture::summary(&report).unpriced_tokens, 5);
    let runs = fixture::runs_daily(&f.admin, &f.tenant).await;
    assert_eq!(
        runs.iter()
            .map(|run| (run.day.as_str(), run.runs))
            .collect::<Vec<_>>(),
        [("2026-09-30", 1)]
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn usage_keeps_deleted_fleet_models() {
    let f = Fixture::shared().await;
    seed_days(&f).await;
    let mut request = fixture::request();
    request.window.from = fixture::instant(BEFORE_MIDNIGHT);
    let report = f.billing.report(&f.tenant, &request).await.unwrap();
    let fleet = fixture::group(&report, dimension::FLEET, Some(f.fleet.as_str()));
    assert_eq!(fleet.label.as_deref(), Some(fixture::FLEET_NAME));
    // The runtime-only slice has no model, so it names none.
    assert_eq!(fleet.models, [fixture::MODEL_A, fixture::MODEL_B]);
    request.filter.work_type = Some(work::AUTHORING_ATTEMPT);
    let filtered = f.billing.report(&f.tenant, &request).await.unwrap();
    let fleet = fixture::group(&filtered, dimension::FLEET, Some(f.fleet.as_str()));
    assert_eq!(fleet.label.as_deref(), Some(fixture::FLEET_NAME));
    assert_eq!((fleet.charged_nanos, fleet.models.len()), (0, 0));
    f.close().await;
}

/// One run crossing UTC midnight, with a runtime-only slice, a setup attempt
/// and a line still unsettled; the fleet identifier has no live fleet row.
async fn seed_days(f: &Fixture) {
    let mut first = Line::runtime(&f.tenant, &f.fleet, 17);
    first.at = BEFORE_MIDNIGHT;
    let mut second = first.next(LineKind::Model, 0, 23);
    second.model = fixture::MODEL_B;
    second.at = MIDNIGHT;
    let mut runtime = first.next(LineKind::Runtime, 60_000, 0);
    runtime.model = "";
    runtime.at = MIDNIGHT;
    let mut setup = Line::setup(&f.tenant, 29);
    setup.at = MIDNIGHT;
    setup.unpriced = 5;
    let pending = Line::runtime(&f.tenant, &f.fleet, 999);
    fixture::seed(&f.admin, std::slice::from_ref(&pending)).await;
    fixture::record(f, &[first, second, runtime, setup]).await;
}

fn days(report: &Report) -> impl Iterator<Item = &Aggregate> {
    report
        .aggregates
        .iter()
        .filter(|row| row.dimension == dimension::DAY)
}

fn assert_additive_totals(report: &Report) {
    let total = fixture::summary(report);
    let quantities: [fn(&Aggregate) -> i64; 6] = [
        |row| row.charged_nanos,
        |row| row.input_tokens,
        |row| row.output_tokens,
        |row| row.runtime_ms,
        |row| row.line_count,
        |row| row.unpriced_tokens,
    ];
    for quantity in quantities {
        assert_eq!(days(report).map(quantity).sum::<i64>(), quantity(total));
    }
}
