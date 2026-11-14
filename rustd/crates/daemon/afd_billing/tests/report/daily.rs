use super::fixture::{self, Fixture, Line};
use afd_billing::report::{Aggregate, Boundary, Filter, dimension};
use afd_billing::sql::work;

const BEFORE_MIDNIGHT: &str = "2026-09-30T23:59:59.999Z";
const MIDNIGHT: &str = "2026-10-01T00:00:00Z";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn daily_spend_reconciles_filtered_snapshot() {
    let f = Fixture::shared().await;
    seed_days(&f).await;
    let mut request = fixture::request(1);
    request.window.from = fixture::instant(BEFORE_MIDNIGHT);
    for filter in [
        Filter::default(),
        Filter {
            fleet: Some(f.fleet.clone()),
            ..Filter::default()
        },
        Filter {
            model: Some(fixture::MODEL_B.to_owned()),
            ..Filter::default()
        },
        Filter {
            work_type: Some(work::AUTHORING_ATTEMPT),
            ..Filter::default()
        },
        Filter {
            fleet: Some(f.fleet.clone()),
            model: Some(fixture::MODEL_B.to_owned()),
            work_type: Some(work::FLEET_LEASE),
        },
    ] {
        request.filter = filter;
        let report = f.billing.report(&f.tenant, &request).await.unwrap();
        assert_additive_totals(&report);
    }
    request.filter = Filter::default();
    let report = f.billing.report(&f.tenant, &request).await.unwrap();
    assert_eq!(fixture::summary(&report).activity_count, 2);
    assert_eq!(days(&report).map(|day| day.activity_count).sum::<i64>(), 3);
    let mut dates: Vec<_> = days(&report)
        .map(|day| day.day.unwrap().to_string())
        .collect();
    dates.sort();
    assert_eq!(dates, ["2026-09-30", "2026-10-01"]);
    assert_eq!(fixture::summary(&report).charged_nanos, 69);
    assert_eq!(fixture::summary(&report).unpriced_tokens, 5);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn daily_spend_ignores_activity_cursor() {
    let f = Fixture::shared().await;
    seed_days(&f).await;
    let mut request = fixture::request(1);
    request.window.from = fixture::instant(BEFORE_MIDNIGHT);
    let first = f.billing.report(&f.tenant, &request).await.unwrap();
    let last = first.activities.first().unwrap();
    request.after = Some(Boundary {
        recorded_at: last.recorded_at,
        work_id: last.work_id.clone(),
        work_type: if last.work_type == work::FLEET_LEASE {
            work::FLEET_LEASE
        } else {
            work::AUTHORING_ATTEMPT
        },
    });
    let next = f.billing.report(&f.tenant, &request).await.unwrap();
    let mut before = first.aggregates;
    let mut after = next.aggregates;
    before.sort_by_key(|row| (row.dimension, row.key.clone(), row.day));
    after.sort_by_key(|row| (row.dimension, row.key.clone(), row.day));
    assert_eq!(before, after);
    assert_ne!(first.activities.first(), next.activities.first());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn usage_keeps_deleted_fleet_models() {
    let f = Fixture::shared().await;
    seed_days(&f).await;
    let mut request = fixture::request(1);
    request.window.from = fixture::instant(BEFORE_MIDNIGHT);
    let report = f.billing.report(&f.tenant, &request).await.unwrap();
    let fleet = report
        .aggregates
        .iter()
        .find(|row| {
            row.dimension == dimension::FLEET && row.key.as_deref() == Some(f.fleet.as_str())
        })
        .unwrap();
    assert_eq!(fleet.label.as_deref(), Some(fixture::FLEET_NAME));
    assert_eq!(fleet.models, [fixture::MODEL_A, fixture::MODEL_B]);
    request.filter.model = Some(fixture::MODEL_B.to_owned());
    let filtered = f.billing.report(&f.tenant, &request).await.unwrap();
    let fleet = filtered
        .aggregates
        .iter()
        .find(|row| {
            row.dimension == dimension::FLEET && row.key.as_deref() == Some(f.fleet.as_str())
        })
        .unwrap();
    assert_eq!(fleet.models, [fixture::MODEL_B]);
    f.close().await;
}

async fn seed_days(f: &Fixture) {
    // The ledger retains a fleet identifier that has no live fleet row.
    let mut first = Line::runtime(&f.tenant, &f.fleet, 17);
    first.at = BEFORE_MIDNIGHT;
    first.settled = Some(MIDNIGHT);
    let mut second = Line::runtime(&f.tenant, &f.fleet, 23);
    second.work_id.clone_from(&first.work_id);
    second.sequence = 1;
    second.model = fixture::MODEL_B;
    second.at = MIDNIGHT;
    second.settled = Some(MIDNIGHT);
    let mut runtime = Line::runtime(&f.tenant, &f.fleet, 0);
    runtime.work_id.clone_from(&first.work_id);
    runtime.sequence = 2;
    runtime.model = "";
    runtime.at = MIDNIGHT;
    runtime.settled = Some(MIDNIGHT);
    let mut setup = Line::setup(&f.tenant, 29);
    setup.at = MIDNIGHT;
    setup.settled = Some(MIDNIGHT);
    setup.unpriced = 5;
    let mut late = Line::runtime(&f.tenant, &f.fleet, 999);
    late.settled = Some(fixture::LATER);
    fixture::seed(&f.admin, &[first, second, runtime, setup, late]).await;
}

fn days(report: &afd_billing::report::Report) -> impl Iterator<Item = &Aggregate> {
    report
        .aggregates
        .iter()
        .filter(|row| row.dimension == dimension::DAY)
}

fn assert_additive_totals(report: &afd_billing::report::Report) {
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
