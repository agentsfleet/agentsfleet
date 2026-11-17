//! Boundary refusals and UTC calendar windows, without a datastore.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::{Input, read};
use afd_billing::report::{BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY};
use afd_billing::sql::work;
use jiff::Timestamp;

const NOW: &str = "2026-10-25T12:34:56.789Z";
const MONTH_START: &str = "2026-10-01T00:00:00Z";
const DAY_START: &str = "2026-10-25T00:00:00Z";
const FLEET: &str = "01924f4e-0000-7000-8000-00000000c4a1";

fn instant(value: &str) -> Timestamp {
    value.parse().expect("fixture instant")
}
fn parsed(query: &str) -> Input {
    read(query, instant(NOW)).expect("valid report query")
}
fn refused(query: &str) -> bool {
    read(query, instant(NOW)).is_err()
}

#[test]
fn default_month_and_today_use_utc_calendar_boundaries() {
    let month = parsed("");
    assert_eq!(month.period, BILLING_PERIOD_MONTH);
    assert_eq!(month.request.window.from, instant(MONTH_START));
    assert_eq!(month.request.window.through, instant(NOW));
    let today = parsed("period=today");
    assert_eq!(today.period, BILLING_PERIOD_TODAY);
    assert_eq!(today.request.window.from, instant(DAY_START));
}

#[test]
fn cutoff_offsets_are_normalized_before_selecting_the_utc_day() {
    let input = parsed("period=today&through_at=2026-10-25T01:00:00%2B05:30");
    assert_eq!(input.request.window.from, instant("2026-10-24T00:00:00Z"));
    assert_eq!(
        input.request.window.through,
        instant("2026-10-24T19:30:00Z")
    );
}

#[test]
fn fleet_and_setup_filters_select_their_work() {
    let input = parsed(&format!("fleet_id={FLEET}&activity_type=runtime"));
    assert_eq!(
        input.request.filter.fleet.map(String::from).as_deref(),
        Some(FLEET)
    );
    assert_eq!(input.request.filter.work_type, Some(work::FLEET_LEASE));
    assert_eq!(
        parsed("activity_type=setup").request.filter.work_type,
        Some(work::AUTHORING_ATTEMPT)
    );
}

#[test]
fn a_removed_model_filter_is_refused_rather_than_widening_the_report() {
    for query in [
        "model=gpt-x",
        "model=%ZZ",
        "model=private%00text",
        "period=today&model=a",
    ] {
        assert!(refused(query), "{query}");
    }
}

#[test]
fn unknown_periods_and_activity_types_are_not_silently_ignored() {
    for query in [
        "period=year",
        "period=Month",
        "period=monthx",
        "activity_type=other",
    ] {
        assert!(refused(query), "{query}");
    }
}

#[test]
fn future_finer_than_millisecond_and_unreadable_cutoffs_are_refused() {
    for value in [
        "2026-10-26T00:00:00Z",
        "2026-10-25T00:00:00.000001Z",
        "yesterday",
        "1792886400000",
    ] {
        assert!(refused(&format!("through_at={value}")), "{value}");
    }
}

#[test]
fn invalid_fleet_identifiers_are_refused_before_a_database_cast() {
    for value in [
        "anything",
        "01924f4e-0000-4000-8000-00000000c4a1",
        "01924F4E-0000-7000-8000-00000000C4A1",
    ] {
        assert!(refused(&format!("fleet_id={value}")), "{value}");
    }
}

mod calendar;
