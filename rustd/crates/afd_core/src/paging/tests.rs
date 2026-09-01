//! Cursor round trips, page bounds and the refusals each one earns.
//!
//! Lifted out of `paging.rs` at the file cap — the first cut the length rule
//! asks for, and the one that frees the most lines for the least risk.

use super::*;

/// A two-ordering resource, standing in for the api-key list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByName {
    CreatedDescending,
    NameAscending,
}

impl SortOrder for ByName {
    const DEFAULT: Self = Self::CreatedDescending;

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "-created_at" => Some(Self::CreatedDescending),
            "key_name" => Some(Self::NameAscending),
            _ => None,
        }
    }

    fn order_by(self) -> &'static str {
        match self {
            Self::CreatedDescending => "created_at DESC, id DESC",
            Self::NameAscending => "key_name ASC, id ASC",
        }
    }

    fn comparator(self) -> Comparator {
        match self {
            Self::CreatedDescending => Comparator::Descending,
            Self::NameAscending => Comparator::Ascending,
        }
    }

    fn boundary(self) -> BoundaryKind {
        match self {
            Self::CreatedDescending => BoundaryKind::Timestamp,
            Self::NameAscending => BoundaryKind::Text,
        }
    }
}

/// A query string as a lookup, for the parser's closure.
fn query<'p>(pairs: &'p [(&'p str, &'static str)]) -> impl Fn(&str) -> Option<&'static str> + 'p {
    move |wanted| {
        pairs
            .iter()
            .find(|(key, _)| *key == wanted)
            .map(|(_, value)| *value)
    }
}

#[test]
fn a_timestamp_cursor_round_trips_through_the_zig_spelling() {
    let cursor = Cursor::Timestamp {
        at_ms: 1_744_000_000_000,
        id: "019abc".to_owned(),
    };
    assert_eq!(cursor.to_string(), "1744000000000:019abc");
    assert_eq!(Cursor::parse("1744000000000:019abc"), Ok(cursor));
}

#[test]
fn a_text_cursor_survives_a_value_holding_the_separator() {
    // The whole reason the text form is encoded: a key named `a:b` would
    // otherwise split the cursor in the wrong place and seek from `a`.
    let cursor = Cursor::Text {
        value: "a:b".to_owned(),
        id: "019abc".to_owned(),
    };
    let wire = cursor.to_string();
    assert_eq!(wire, "s:YTpi:019abc");
    assert_eq!(Cursor::parse(&wire), Ok(cursor));
}

#[test]
fn a_cursor_that_is_not_one_is_refused_without_saying_why() {
    for raw in [
        "",
        "1744000000000",
        "1744000000000:",
        "abc:019abc",
        "s:019abc",
        "s:!!!!:019abc",
        "s:YTpi:",
    ] {
        assert_eq!(Cursor::parse(raw), Err(InvalidCursor), "cursor {raw:?}");
    }
}

#[test]
fn the_defaults_apply_when_the_caller_names_nothing() {
    assert_eq!(
        Page::<ByName>::parse(query(&[])).ok(),
        Some(Page {
            cursor: None,
            limit: DEFAULT_LIMIT,
            sort: ByName::CreatedDescending,
        })
    );
}

#[test]
fn the_limit_is_bounded_at_both_ends() {
    for (raw, expected) in [("1", Ok(1)), ("100", Ok(100))] {
        let page = Page::<ByName>::parse(query(&[(QUERY_LIMIT, raw)]));
        assert_eq!(page.map(|page| page.limit), expected, "limit {raw:?}");
    }
    for raw in ["0", "101", "-1", "", "ten", "1e2"] {
        assert_eq!(
            Page::<ByName>::parse(query(&[(QUERY_LIMIT, raw)])).err(),
            Some(PagingRefusal::Limit),
            "limit {raw:?}"
        );
    }
}

#[test]
fn an_unknown_sort_is_refused_rather_than_coerced_to_the_default() {
    // Coercion is the dangerous half: a caller asking for `key_name` and
    // silently getting `-created_at` pages through data they did not ask
    // for, and every page looks fine.
    assert_eq!(
        Page::<ByName>::parse(query(&[(QUERY_SORT, "id")])).err(),
        Some(PagingRefusal::Sort)
    );
    assert_eq!(
        Page::<ByName>::parse(query(&[(QUERY_SORT, "created_at DESC; DROP TABLE")])).err(),
        Some(PagingRefusal::Sort)
    );
}

#[test]
fn a_cursor_from_another_ordering_is_refused() {
    // Both of these are well-formed cursors. Neither matches the sort it is
    // presented with, and resuming either would page a third ordering that
    // is not the one asked for or the one the cursor came from.
    let timestamp_under_name = query(&[
        (QUERY_SORT, "key_name"),
        (QUERY_STARTING_AFTER, "1744000000000:019abc"),
    ]);
    assert_eq!(
        Page::<ByName>::parse(timestamp_under_name).err(),
        Some(PagingRefusal::Cursor)
    );

    let text_under_timestamp = query(&[
        (QUERY_SORT, "-created_at"),
        (QUERY_STARTING_AFTER, "s:YTpi:019abc"),
    ]);
    assert_eq!(
        Page::<ByName>::parse(text_under_timestamp).err(),
        Some(PagingRefusal::Cursor)
    );
}

#[test]
fn a_matching_cursor_is_taken() {
    assert_eq!(
        Page::<ByName>::parse(query(&[
            (QUERY_SORT, "key_name"),
            (QUERY_STARTING_AFTER, "s:YTpi:019abc"),
        ]))
        .ok(),
        Some(Page {
            cursor: Some(Cursor::Text {
                value: "a:b".to_owned(),
                id: "019abc".to_owned(),
            }),
            limit: DEFAULT_LIMIT,
            sort: ByName::NameAscending,
        })
    );
}

#[test]
fn the_retired_offset_parameters_are_refused_not_ignored() {
    for retired in [QUERY_PAGE, QUERY_PAGE_SIZE] {
        assert_eq!(
            Page::<ByName>::parse(query(&[(retired, "2")])).err(),
            Some(PagingRefusal::OffsetParametersRetired),
            "parameter {retired}"
        );
    }
}

#[test]
fn every_ordering_breaks_its_tie_on_the_row_id() {
    // Without the id in the ORDER BY, two rows sharing a creation
    // millisecond have no defined order between them, and a seek past that
    // boundary drops one of them from every page it could appear on.
    for sort in [ByName::CreatedDescending, ByName::NameAscending] {
        assert!(
            sort.order_by().contains("id"),
            "{sort:?} orders without a tiebreak"
        );
    }
}

/// Every refusal reads as its own repair, and each is a sentence.
///
/// The type's own doc gives the reason: three reasons rather than one because
/// a caller ACTS differently on each — stop sending a retired parameter, ask
/// for fewer rows, or start the walk again. They share `UZ-REQ-001`, so the
/// sentence is the only thing that tells them apart, and two arms answering one
/// string would send a caller to fix the wrong parameter.
#[test]
fn every_paging_refusal_names_its_own_repair() {
    let refusals = [
        PagingRefusal::OffsetParametersRetired,
        PagingRefusal::Limit,
        PagingRefusal::Sort,
        PagingRefusal::Cursor,
    ];

    let mut seen = std::collections::BTreeSet::new();
    for refusal in refusals {
        let detail = refusal.detail();
        assert!(!detail.is_empty(), "{refusal:?} answers no sentence at all");
        assert!(
            seen.insert(detail),
            "{refusal:?} repeats a sentence another refusal already owns: {detail}"
        );
    }

    // The parameter each one is about, named in the sentence a caller reads.
    // Without this the loop above passes on four distinct strings that could
    // each be about the wrong field.
    assert!(
        PagingRefusal::OffsetParametersRetired
            .detail()
            .contains("page_size")
    );
    assert!(PagingRefusal::Limit.detail().contains("limit"));
    assert!(PagingRefusal::Sort.detail().contains("sort"));
    assert!(PagingRefusal::Cursor.detail().contains("starting_after"));
}
