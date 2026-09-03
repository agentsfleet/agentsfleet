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

#[test]
fn a_page_cursor_names_the_last_returned_row() {
    let id = fleet_id().to_string();
    let page = FleetPage {
        rows: vec![FleetRow {
            id: id.clone(),
            name: "reviewer".to_owned(),
            status: FleetStatus::Active,
            created_at_ms: 42,
            updated_at_ms: 43,
            triggers: None,
            events_processed: 7,
            budget_used_nanos: 11,
        }],
        more: true,
    };

    let response = page_response(&page);

    assert_eq!(response.total, 1);
    assert_eq!(response.items[0].id, id);
    let parsed = parse_cursor(response.next_cursor.as_deref())
        .unwrap_or_else(|error| panic!("emitted cursor parses: {error:?}"))
        .unwrap_or_else(|| panic!("a page with more rows emits a cursor"));
    assert_eq!(parsed.created_at_ms, 42);
    assert_eq!(parsed.id.as_str(), id);
}

#[test]
fn an_install_reply_builds_each_webhook_from_the_deployment() {
    let installed = Installed {
        id: fleet_id(),
        name: "reviewer".to_owned(),
        status: FleetStatus::Active,
        webhook_sources: vec!["github".into(), "slack".into()],
    };

    let response = installed_response(&installed, "https://api.example.test");

    assert_eq!(response.fleet_id, installed.id.as_str());
    assert_eq!(response.webhook_urls.len(), 2);
    assert_eq!(
        response.webhook_urls[0].url,
        format!(
            "https://api.example.test/v1/webhooks/{}/github",
            installed.id.as_str()
        )
    );
}
