//! The fleets verb's excerpt bound and its list envelope.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use super::{EXCERPT_MAX_BYTES, FleetsPage, excerpt};

/// A 10 KiB answer whose three-byte character starts at byte
/// 1022, spanning byte 1024, is cut before that character, never inside it.
#[test]
fn fleets_read_excerpt_is_bounded() {
    let mut answer = "a".repeat(1022);
    answer.push('€');
    answer.push_str(&"b".repeat(10 * 1024 - answer.len()));
    let cut = excerpt(&answer);
    assert_eq!(cut.len(), 1022);
    assert_eq!(cut, "a".repeat(1022));
    assert!(EXCERPT_MAX_BYTES >= cut.len());
}

#[test]
fn a_short_text_is_its_own_excerpt() {
    assert_eq!(excerpt("all clear"), "all clear");
    assert_eq!(excerpt(""), "");
}

#[test]
fn a_text_at_the_bound_is_kept_whole() {
    let exact = "c".repeat(EXCERPT_MAX_BYTES);
    assert_eq!(excerpt(&exact).len(), EXCERPT_MAX_BYTES);
}

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
