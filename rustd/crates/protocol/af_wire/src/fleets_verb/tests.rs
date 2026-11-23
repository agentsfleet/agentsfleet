//! The fleets verb's list envelope.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use super::FleetsPage;

/// Every list carries one envelope, `total` and `next_cursor` included as
/// null (HTTP-LIST).
#[test]
fn an_empty_page_spells_both_nullable_keys() {
    let page = FleetsPage {
        items: Vec::new(),
        total: None,
        next_cursor: None,
    };
    let body = serde_json::to_string(&page).expect("a page serializes");
    assert_eq!(body, r#"{"items":[],"total":null,"next_cursor":null}"#);
}
