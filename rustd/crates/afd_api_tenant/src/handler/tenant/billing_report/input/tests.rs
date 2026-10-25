//! Boundary refusals and stable report continuation, without a datastore.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::{Input, read};
use crate::handler::tenant::billing_report::cursor::{Cursor, Position};
use afd_billing::report::{BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY};
use afd_billing::sql::work;
use afd_core::id::Uuid7;
use jiff::Timestamp;

const TENANT: &str = "01924f4e-0000-7000-8000-00000000c4a1";
const FOREIGN: &str = "01924f4e-0000-7000-8000-00000000c4a2";
const ACTIVITY: &str = "01924f4e-0000-7000-8000-00000000c4a3";
const NOW: &str = "2026-10-25T12:34:56.789Z";
const MONTH_START: &str = "2026-10-01T00:00:00Z";
const DAY_START: &str = "2026-10-25T00:00:00Z";

fn instant(value: &str) -> Timestamp {
    value.parse().expect("fixture instant")
}
fn tenant(value: &str) -> Uuid7 {
    Uuid7::parse(value).expect("fixture identifier")
}
fn parsed(query: &str) -> Input {
    read(query, &tenant(TENANT), None, instant(NOW)).expect("valid report query")
}
fn token(input: &Input) -> String {
    Cursor::render(
        &input.scope,
        Position::Activity {
            at: NOW.to_owned(),
            id: ACTIVITY.to_owned(),
            work_type: work::FLEET_LEASE.to_owned(),
        },
    )
}

#[test]
fn default_month_and_today_use_utc_calendar_boundaries() {
    let month = parsed("");
    assert_eq!(month.scope.period, BILLING_PERIOD_MONTH);
    assert_eq!(month.request.window.from, instant(MONTH_START));
    assert_eq!(month.request.window.through, instant(NOW));
    assert_eq!(month.request.limit, afd_core::paging::DEFAULT_LIMIT);
    let today = parsed("period=today");
    assert_eq!(today.scope.period, BILLING_PERIOD_TODAY);
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
fn report_limits_keep_the_shared_edges_and_refuse_other_spellings() {
    for count in [1, afd_core::paging::MAX_LIMIT] {
        assert_eq!(parsed(&format!("limit={count}")).request.limit, count);
    }
    for query in [
        "limit=0",
        "limit=101",
        "limit=-1",
        "limit=lots",
        "limit=1.5",
    ] {
        assert!(
            read(query, &tenant(TENANT), None, instant(NOW)).is_err(),
            "{query}"
        );
    }
}

#[test]
fn unknown_periods_and_activity_types_are_not_silently_ignored() {
    for query in [
        "period=week",
        "period=Month",
        "period=monthx",
        "activity_type=other",
    ] {
        assert!(
            read(query, &tenant(TENANT), None, instant(NOW)).is_err(),
            "{query}"
        );
    }
}

#[test]
fn malformed_escapes_and_nul_models_cannot_select_an_unfiltered_report() {
    for query in ["model=%ZZ", "model=%FF", "model=private%00text"] {
        assert!(
            read(query, &tenant(TENANT), None, instant(NOW)).is_err(),
            "{query}"
        );
    }
}

#[test]
fn oversized_cursor_and_model_are_refused_before_decoding() {
    for query in [
        format!("starting_after={}", "a".repeat(super::CURSOR_MAX_BYTES + 1)),
        format!(
            "model={}",
            "a".repeat(afd_api_wire::admin::MODEL_ID_MAX_BYTES + 1)
        ),
    ] {
        assert!(read(&query, &tenant(TENANT), None, instant(NOW)).is_err());
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
        assert!(
            read(
                &format!("through_at={value}"),
                &tenant(TENANT),
                None,
                instant(NOW)
            )
            .is_err(),
            "{value}"
        );
    }
}

#[test]
fn invalid_fleet_identifiers_are_refused_before_a_database_cast() {
    for value in [
        "anything",
        "01924f4e-0000-4000-8000-00000000c4a1",
        "01924F4E-0000-7000-8000-00000000C4A1",
    ] {
        assert!(
            read(
                &format!("fleet_id={value}"),
                &tenant(TENANT),
                None,
                instant(NOW)
            )
            .is_err(),
            "{value}"
        );
    }
}

#[test]
fn a_continuation_keeps_its_cutoff_and_every_ordering_key() {
    let original = parsed("model=fixture&limit=1");
    let query = format!("model=fixture&limit=1&starting_after={}", token(&original));
    let continued = read(
        &query,
        &tenant(TENANT),
        None,
        instant("2026-10-26T00:00:00Z"),
    )
    .expect("same report can continue tomorrow");
    assert_eq!(continued.scope, original.scope);
    let after = continued.request.after.expect("activity boundary");
    assert_eq!(after.recorded_at, instant(NOW));
    assert_eq!(after.work_id.as_str(), ACTIVITY);
    assert_eq!(after.work_type, work::FLEET_LEASE);
}

#[test]
fn cursor_scope_refuses_changed_filters_size_cutoff_and_tenant() {
    let original = parsed("");
    let token = token(&original);
    for suffix in [
        "period=today",
        "model=other",
        "limit=1",
        "activity_type=setup",
        "through_at=2026-10-24T00:00:00Z",
    ] {
        assert!(
            read(
                &format!("starting_after={token}&{suffix}"),
                &tenant(TENANT),
                None,
                instant(NOW)
            )
            .is_err(),
            "{suffix}"
        );
    }
    assert!(
        read(
            &format!("starting_after={token}"),
            &tenant(FOREIGN),
            None,
            instant(NOW)
        )
        .is_err()
    );
    assert!(
        read(
            &format!("starting_after={token}&fleet_id={ACTIVITY}"),
            &tenant(TENANT),
            None,
            instant(NOW)
        )
        .is_err()
    );
}

#[test]
fn detail_filters_validate_the_identifier_and_retain_the_selected_work_type() {
    assert!(
        read(
            "activity_type=setup",
            &tenant(TENANT),
            Some("broken"),
            instant(NOW)
        )
        .is_err()
    );
    let input = read(
        "activity_type=setup",
        &tenant(TENANT),
        Some(ACTIVITY),
        instant(NOW),
    )
    .expect("valid setup detail");
    assert_eq!(
        input.request.filter.work_type,
        Some(work::AUTHORING_ATTEMPT)
    );
}

#[test]
fn activity_and_charge_continuations_are_not_interchangeable() {
    let input = parsed("");
    let wrong = Cursor::render(&input.scope, Position::Charge { sequence: 1 });
    assert!(
        read(
            &format!("starting_after={wrong}"),
            &tenant(TENANT),
            None,
            instant(NOW)
        )
        .is_err()
    );
    let input = read(
        "activity_type=setup",
        &tenant(TENANT),
        Some(ACTIVITY),
        instant(NOW),
    )
    .expect("valid setup detail");
    let wrong = Cursor::render(
        &input.scope,
        Position::Activity {
            at: NOW.to_owned(),
            id: ACTIVITY.to_owned(),
            work_type: work::AUTHORING_ATTEMPT.to_owned(),
        },
    );
    assert!(
        read(
            &format!("activity_type=setup&starting_after={wrong}"),
            &tenant(TENANT),
            Some(ACTIVITY),
            instant(NOW)
        )
        .is_err()
    );
}

#[test]
fn charge_cursor_cannot_cross_activities_or_name_a_negative_sequence() {
    let input = read(
        "activity_type=setup",
        &tenant(TENANT),
        Some(ACTIVITY),
        instant(NOW),
    )
    .expect("valid setup detail");
    let token = Cursor::render(&input.scope, Position::Charge { sequence: 1 });
    assert!(
        read(
            &format!("activity_type=setup&starting_after={token}"),
            &tenant(TENANT),
            Some(FOREIGN),
            instant(NOW)
        )
        .is_err()
    );
    let negative = Cursor::render(&input.scope, Position::Charge { sequence: -1 });
    assert!(
        read(
            &format!("activity_type=setup&starting_after={negative}"),
            &tenant(TENANT),
            Some(ACTIVITY),
            instant(NOW)
        )
        .is_err()
    );
}
