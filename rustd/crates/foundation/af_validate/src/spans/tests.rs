//! The span rule at its edges.

use super::{OVERLAPPING, REVERSED, Span, disjoint};

const fn span(key: u8, start: u8, end: Option<u8>) -> Span<u8, u8> {
    Span { key, start, end }
}

fn message(spans: &[Span<u8, u8>]) -> Option<String> {
    disjoint(spans.iter().copied())
        .err()
        .map(|error| error.message().to_owned())
}

#[test]
fn spans_that_touch_or_differ_by_key_are_disjoint() {
    assert_eq!(message(&[]), None);
    assert_eq!(message(&[span(0, 1, Some(5)), span(0, 5, None)]), None);
    assert_eq!(message(&[span(0, 1, None), span(1, 1, None)]), None);
}

#[test]
fn a_shared_instant_under_one_key_overlaps() {
    assert_eq!(
        message(&[span(0, 5, None), span(0, 1, Some(6))]).as_deref(),
        Some(OVERLAPPING)
    );
    assert_eq!(
        message(&[span(0, 1, None), span(0, 9, Some(10))]).as_deref(),
        Some(OVERLAPPING)
    );
}

#[test]
fn a_span_must_end_after_it_starts() {
    for end in [1, 0] {
        assert_eq!(message(&[span(0, 1, Some(end))]).as_deref(), Some(REVERSED));
    }
}
