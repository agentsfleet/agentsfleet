//! Real settled-report reads: full-period sums, stable attribution and bounded pages.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "live-schema assertions fail loudly"
)]

use afd_billing::biller::WorkKey;
use afd_billing::report::{Boundary, dimension};
use afd_billing::sql::{posture, work};
use std::collections::HashSet;

#[path = "report/concurrency.rs"]
mod concurrency;
#[path = "report/failures.rs"]
mod failures;
#[path = "report/fixture.rs"]
mod fixture;
#[path = "report/plans.rs"]
mod plans;
use fixture::{Fixture, Line};

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn settled_report_totals_cover_more_than_one_activity_page() {
    let f = Fixture::shared().await;
    let mut rows: Vec<_> = (0..31)
        .map(|_| Line::runtime(&f.tenant, &f.fleet, 17))
        .collect();
    rows.push(Line::setup(&f.tenant, 29));
    rows.push(Line::runtime(&f.foreign, &f.fleet, 999));
    fixture::seed(&f.admin, &rows).await;
    let before = fixture::ledger(&f.admin, &f.tenant).await;
    let report = f
        .billing
        .report(&f.tenant, &fixture::request(10))
        .await
        .unwrap();
    let total = fixture::summary(&report);
    assert_eq!(total.charged_nanos, 31 * 17 + 29);
    assert_eq!((total.activity_count, total.line_count), (32, 32));
    assert_eq!(total.input_tokens, 32 * fixture::INPUT);
    assert_eq!(total.output_tokens, 32 * fixture::OUTPUT);
    assert_eq!(total.runtime_ms, 32 * fixture::RUNTIME);
    assert_eq!(report.activities.len(), 11, "one bounded lookahead");
    assert_eq!(fixture::ledger(&f.admin, &f.tenant).await, before);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn tied_activity_times_page_without_skips_or_duplicates() {
    let f = Fixture::shared().await;
    let mut rows: Vec<_> = (0..17)
        .map(|_| Line::runtime(&f.tenant, &f.fleet, 1))
        .collect();
    let mut setup = Line::setup(&f.tenant, 2);
    setup.work_id.clone_from(&rows.first().unwrap().work_id); // Work kind is a real final tie-breaker.
    rows.push(setup);
    fixture::seed(&f.admin, &rows).await;
    let expected: HashSet<_> = rows
        .iter()
        .map(|line| (line.work_type.to_owned(), line.work_id.clone()))
        .collect();
    let mut seen = HashSet::new();
    let mut request = fixture::request(3);
    for _ in 0..7 {
        let report = f.billing.report(&f.tenant, &request).await.unwrap();
        assert_eq!(fixture::summary(&report).activity_count, 18);
        for row in report.activities.iter().take(3) {
            assert!(
                seen.insert((row.work_type.clone(), row.work_id.clone())),
                "duplicate activity"
            );
        }
        if report.activities.len() <= 3 {
            break;
        }
        let last = report.activities.get(2).unwrap();
        request.after = Some(Boundary {
            recorded_at: last.recorded_at,
            work_id: afd_core::id::Uuid7::parse(&last.work_id).unwrap(),
            work_type: if last.work_type == work::FLEET_LEASE {
                work::FLEET_LEASE
            } else {
                work::AUTHORING_ATTEMPT
            },
        });
    }
    assert_eq!(seen, expected);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn pending_outside_period_and_later_settlements_are_excluded() {
    let f = Fixture::shared().await;
    let mut pending = Line::setup(&f.tenant, 999);
    pending.settled = None;
    let mut old = Line::setup(&f.tenant, 888);
    old.at = fixture::BEFORE;
    old.settled = Some(fixture::BEFORE);
    let mut late = Line::setup(&f.tenant, 777);
    late.settled = Some(fixture::LATER);
    let mut future = Line::setup(&f.tenant, 666);
    future.at = fixture::LATER;
    future.settled = Some(fixture::LATER);
    let mut unknown = Line::setup(&f.tenant, 0);
    unknown.posture = posture::SELF_MANAGED;
    unknown.unpriced = 5;
    fixture::seed(&f.admin, &[pending, old, late, future, unknown]).await;
    let report = f
        .billing
        .report(&f.tenant, &fixture::request(10))
        .await
        .unwrap();
    let summary = fixture::summary(&report);
    assert_eq!(
        (
            summary.line_count,
            summary.charged_nanos,
            summary.unpriced_tokens
        ),
        (1, 0, 5)
    );
    assert_eq!(
        report.activities.first().unwrap().postures,
        [posture::SELF_MANAGED]
    );
    assert_eq!(fixture::ledger(&f.admin, &f.tenant).await, (5, 3330, 4));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn exact_filters_preserve_unfiltered_choices_and_deleted_fleet_labels() {
    let f = Fixture::shared().await;
    let mut second = Line::runtime(&f.tenant, &fixture::id(), 23);
    second.model = fixture::MODEL_B;
    fixture::seed(
        &f.admin,
        &[
            Line::runtime(&f.tenant, &f.fleet, 17),
            second,
            Line::setup(&f.tenant, 29),
        ],
    )
    .await;
    let mut request = fixture::request(10);
    request.filter.fleet = Some(f.fleet.clone());
    request.filter.model = Some(fixture::MODEL_A.to_owned());
    let report = f.billing.report(&f.tenant, &request).await.unwrap();
    assert_eq!(fixture::summary(&report).charged_nanos, 17);
    assert_eq!(
        report.activities.first().unwrap().fleet_name.as_deref(),
        Some(fixture::FLEET_NAME)
    );
    let models: HashSet<_> = report
        .aggregates
        .iter()
        .filter(|row| row.dimension == dimension::MODEL)
        .filter_map(|row| row.key.as_deref())
        .collect();
    assert_eq!(models, HashSet::from([fixture::MODEL_A, fixture::MODEL_B]));
    request.filter.fleet = None;
    request.filter.work_type = Some(work::AUTHORING_ATTEMPT);
    request.filter.model = Some(fixture::MODEL_B.to_owned());
    let setup = f.billing.report(&f.tenant, &request).await.unwrap();
    assert_eq!(fixture::summary(&setup).charged_nanos, 29);
    assert_eq!(setup.activities.len(), 1);
    assert_eq!(setup.activities.first().unwrap().fleet_name, None);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn individual_charges_keep_tenant_filters_and_sequence_bounds() {
    let f = Fixture::shared().await;
    let first = Line::runtime(&f.tenant, &f.fleet, 17);
    let work = WorkKey::FleetLease(afd_core::id::Uuid7::parse(&first.work_id).unwrap());
    let mut second = Line::runtime(&f.tenant, &f.fleet, 23);
    second.work_id.clone_from(&first.work_id);
    second.sequence = 1;
    second.model = fixture::MODEL_B;
    fixture::seed(&f.admin, &[first, second]).await;
    let mut request = fixture::request(1);
    let page = f
        .billing
        .activity_charges(&f.tenant, &work, &request, None)
        .await
        .unwrap();
    assert_first_charge_page(&page);
    let next = f
        .billing
        .activity_charges(&f.tenant, &work, &request, Some(0))
        .await
        .unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next.first().unwrap().charged_nanos, 23);
    let foreign = f
        .billing
        .activity_charges(&f.foreign, &work, &request, None)
        .await
        .unwrap();
    assert_eq!(foreign.len(), 0);
    request.filter.model = Some(fixture::MODEL_A.to_owned());
    let filtered = f
        .billing
        .activity_charges(&f.tenant, &work, &request, Some(0))
        .await
        .unwrap();
    assert_eq!(filtered.len(), 0);
    request.filter.model = None;
    request.filter.fleet = Some(fixture::id());
    let wrong_fleet = f
        .billing
        .activity_charges(&f.tenant, &work, &request, None)
        .await
        .unwrap();
    assert_eq!(wrong_fleet.len(), 0);
    f.close().await;
}

fn assert_first_charge_page(page: &[afd_billing::report::Charge]) {
    assert_eq!(page.len(), 2);
    assert_eq!(
        (
            page.first().unwrap().sequence,
            page.get(1).unwrap().sequence
        ),
        (0, 1)
    );
}
