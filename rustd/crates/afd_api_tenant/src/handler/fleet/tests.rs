//! What the list and install verbs render, for the shapes their rows can take.
//!
//! Split out of [`super`], which is at the length cap. Every case here reads a
//! value already fetched — a page, a row, an install outcome — so none of them
//! needs a pool, and the router suite keeps the refusals.

#![expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "fixed test fixtures fail loudly when their contract changes"
)]

use super::*;
use afd_fleet_lifecycle::FleetStatus;

fn fleet_id() -> Uuid7 {
    Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010")
        .unwrap_or_else(|error| panic!("fixture id is canonical: {error}"))
}

/// When the fixture fleet was made.
const CREATED_AT: jiff::Timestamp = jiff::Timestamp::constant(1_700_000_000, 42_000_000);

/// One listed fleet carrying `description`.
fn row(description: Option<&str>) -> FleetRow {
    FleetRow {
        id: fleet_id().to_string(),
        name: "reviewer".to_owned(),
        status: FleetStatus::Active,
        created_at: CREATED_AT,
        updated_at: CREATED_AT,
        description: description.map(str::to_owned),
        triggers: None,
        events_processed: 7,
        budget_used_nanos: 11,
    }
}

#[test]
fn a_summary_carries_the_saved_purpose_and_publishes_its_absence_as_null() {
    for (description, expected) in [
        (
            Some("Reviews pull requests."),
            serde_json::json!("Reviews pull requests."),
        ),
        (None, serde_json::Value::Null),
    ] {
        let page = FleetPage {
            rows: vec![row(description)],
            more: false,
        };

        let wire = serde_json::to_value(page_response(&page))
            .unwrap_or_else(|error| panic!("a page serializes: {error}"));

        assert_eq!(
            wire["items"][0]["description"], expected,
            "the purpose is listed, and a fleet with no saved setup lists null rather than omitting it"
        );
    }
}

#[test]
fn a_page_cursor_names_the_last_returned_row() {
    let id = fleet_id().to_string();
    let page = FleetPage {
        rows: vec![row(None)],
        more: true,
    };

    let response = page_response(&page);

    assert_eq!(response.total, None);
    assert_eq!(response.items[0].id, id);
    let parsed = parse_cursor(response.next_cursor.as_deref())
        .unwrap_or_else(|error| panic!("emitted cursor parses: {error:?}"))
        .unwrap_or_else(|| panic!("a page with more rows emits a cursor"));
    assert_eq!(parsed.created_at, CREATED_AT);
    assert_eq!(parsed.id.as_str(), id);
}
