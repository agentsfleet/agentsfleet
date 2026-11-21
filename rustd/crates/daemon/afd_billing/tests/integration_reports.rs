//! Real settled-report reads over the daily rollup the settle transaction
//! writes: full-period sums, stable attribution and the rollup's guarantees.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "live-schema assertions fail loudly"
)]
#![expect(
    clippy::print_stdout,
    reason = "the lane prints its measured shape for the operator"
)]

use af_wire::usage::LineKind;
use afd_billing::report::dimension;
use afd_billing::sql::{posture, work};
use std::collections::HashSet;

#[path = "report/concurrency.rs"]
mod concurrency;
#[path = "report/cut_proxy.rs"]
mod cut_proxy;
#[path = "report/daily.rs"]
mod daily;
#[path = "report/failures.rs"]
mod failures;
#[path = "report/fixture.rs"]
mod fixture;
#[path = "report/funds_plan.rs"]
mod funds_plan;
#[path = "report/plans.rs"]
mod plans;
#[path = "report/reconcile.rs"]
mod reconcile;
#[path = "report/rollup.rs"]
mod rollup;
#[path = "report/settle_races.rs"]
mod settle_races;
#[path = "report/settle_rollup.rs"]
mod settle_rollup;
use fixture::{Fixture, Line};

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn settled_report_totals_cover_the_whole_period() {
    let f = Fixture::shared().await;
    let mut rows: Vec<_> = (0..31)
        .map(|_| Line::runtime(&f.tenant, &f.fleet, 17))
        .collect();
    rows.push(Line::setup(&f.tenant, 29));
    rows.push(Line::runtime(&f.foreign, &f.fleet, 999));
    fixture::record(&f, &rows).await;
    let before = fixture::ledger(&f.admin, &f.tenant).await;
    let report = f
        .billing
        .report(&f.tenant, &fixture::request())
        .await
        .unwrap();
    let total = fixture::summary(&report);
    assert_eq!(total.charged_nanos, 31 * 17 + 29);
    // Setup work has no runtime slice, so only the 31 fleet runs count.
    assert_eq!((total.activity_count, total.line_count), (Some(31), 32));
    assert_eq!(total.input_tokens, 32 * fixture::INPUT);
    assert_eq!(total.output_tokens, 32 * fixture::OUTPUT);
    assert_eq!(total.runtime_ms, 32 * fixture::RUNTIME);
    assert_eq!(
        fixture::balance(&f.admin, &f.tenant).await,
        fixture::BALANCE - (31 * 17 + 29)
    );
    assert_eq!(before, (32, 31 * 17 + 29, 32));
    assert_eq!(fixture::ledger(&f.admin, &f.tenant).await, before);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn pending_and_out_of_period_lines_are_excluded() {
    let f = Fixture::shared().await;
    let pending = Line::setup(&f.tenant, 999);
    let mut old = Line::setup(&f.tenant, 888);
    old.at = fixture::BEFORE;
    let mut next_day = Line::setup(&f.tenant, 777);
    next_day.at = "2026-10-26T00:00:00Z";
    // The report covers whole UTC days, so a line after the cutoff on its day counts.
    let mut same_day = Line::setup(&f.tenant, 666);
    same_day.at = fixture::LATER;
    let mut unknown = Line::setup(&f.tenant, 0);
    unknown.posture = posture::SELF_MANAGED;
    unknown.unpriced = 5;
    fixture::seed(&f.admin, std::slice::from_ref(&pending)).await;
    fixture::record(&f, &[old, next_day, same_day, unknown]).await;
    let report = f
        .billing
        .report(&f.tenant, &fixture::request())
        .await
        .unwrap();
    let summary = fixture::summary(&report);
    assert_eq!(
        (
            summary.line_count,
            summary.charged_nanos,
            summary.unpriced_tokens
        ),
        (2, 666, 5)
    );
    let days: Vec<_> = report
        .aggregates
        .iter()
        .filter(|row| row.dimension == dimension::DAY)
        .map(|row| row.day.unwrap().to_string())
        .collect();
    assert_eq!(days, ["2026-10-25"]);
    assert_eq!(fixture::ledger(&f.admin, &f.tenant).await, (5, 3330, 4));
    assert_eq!(
        fixture::balance(&f.admin, &f.tenant).await,
        fixture::BALANCE - (888 + 777 + 666)
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn exact_filters_preserve_unfiltered_choices_and_deleted_fleet_labels() {
    let f = Fixture::shared().await;
    let other = fixture::id();
    let mut second = Line::runtime(&f.tenant, &other, 23);
    second.model = fixture::MODEL_B;
    second.name = Some("Release notes".to_owned());
    let first = Line::runtime(&f.tenant, &f.fleet, 17);
    let call = first.next(LineKind::Model, 0, 3);
    fixture::record(&f, &[first, call, second, Line::setup(&f.tenant, 29)]).await;
    let mut request = fixture::request();
    request.filter.fleet = Some(f.fleet.clone());
    let report = f.billing.report(&f.tenant, &request).await.unwrap();
    let summary = fixture::summary(&report);
    assert_eq!(
        (summary.charged_nanos, summary.activity_count),
        (20, Some(1))
    );
    // The ledger keeps a fleet identifier that has no live fleet row.
    let fleet = fixture::group(&report, dimension::FLEET, Some(f.fleet.as_str()));
    assert_eq!(fleet.label.as_deref(), Some(fixture::FLEET_NAME));
    assert_eq!((fleet.charged_nanos, fleet.activity_count), (20, Some(1)));
    let unselected = fixture::group(&report, dimension::FLEET, Some(other.as_str()));
    assert_eq!(unselected.label.as_deref(), Some("Release notes"));
    assert_eq!(
        (
            unselected.charged_nanos,
            unselected.activity_count,
            unselected.models.len()
        ),
        (0, Some(0), 0)
    );
    let fleets: HashSet<_> = report
        .aggregates
        .iter()
        .filter(|row| row.dimension == dimension::FLEET)
        .map(|row| row.key.clone())
        .collect();
    assert_eq!(
        fleets,
        HashSet::from([Some(f.fleet.to_string()), Some(other.to_string()), None])
    );
    let models: HashSet<_> = report
        .aggregates
        .iter()
        .filter(|row| row.dimension == dimension::MODEL)
        .map(|row| (row.key.as_deref(), row.charged_nanos, row.activity_count))
        .collect();
    assert_eq!(
        models,
        HashSet::from([
            (Some(fixture::MODEL_A), 20, None),
            (Some(fixture::MODEL_B), 0, None)
        ])
    );
    request.filter.fleet = None;
    request.filter.work_type = Some(work::SETUP_RUN);
    let setup = f.billing.report(&f.tenant, &request).await.unwrap();
    let summary = fixture::summary(&setup);
    assert_eq!(
        (summary.charged_nanos, summary.activity_count),
        (29, Some(0))
    );
    let private = fixture::group(&setup, dimension::FLEET, None);
    assert_eq!(
        (private.label.as_deref(), private.charged_nanos),
        (None, 29)
    );
    assert_eq!(private.models, [fixture::MODEL_B]);
    f.close().await;
}
