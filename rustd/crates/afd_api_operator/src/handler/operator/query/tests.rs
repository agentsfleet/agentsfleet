//! What the operator lists read off a query string, and what each refuses.

#![expect(clippy::expect_used, reason = "tests inspect canonical query fixtures")]

use super::*;

#[test]
fn page_query_accepts_defaults_and_composite_cursors() {
    let empty = HashMap::new();
    let defaults = page(&empty).expect("defaults are valid");
    assert_eq!(defaults.limit, CEILING.default_rows());
    assert!(defaults.cursor.is_none());

    let mut params = HashMap::new();
    params.insert(QUERY_LIMIT.to_owned(), "100".to_owned());
    let cursor = "1725000000000:0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
    params.insert(QUERY_STARTING_AFTER.to_owned(), cursor.to_owned());
    let parsed = page(&params).expect("the boundary is canonical");
    assert_eq!(parsed.limit, 100);
    assert_eq!(
        format(parsed.cursor.as_ref().expect("cursor exists")),
        cursor
    );
}

#[test]
fn page_query_refuses_retired_and_out_of_range_inputs() {
    for (key, value, detail) in [
        (QUERY_PAGE, "2", DETAIL_RETIRED_PAGE),
        (QUERY_LIMIT, "0", DETAIL_BAD_PAGE),
        (QUERY_LIMIT, "101", DETAIL_BAD_PAGE),
        (QUERY_STARTING_AFTER, "not-a-cursor", DETAIL_BAD_PAGE),
    ] {
        let params = HashMap::from([(key.to_owned(), value.to_owned())]);
        assert_eq!(page(&params).err(), Some(detail));
    }
}

#[test]
fn lease_query_parses_independent_filters_and_refuses_each_bad_dimension() {
    let cursor = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
    let workspace = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a7d";
    let params = HashMap::from([
        (QUERY_LIMIT.to_owned(), "100".to_owned()),
        (QUERY_STARTING_AFTER.to_owned(), cursor.to_owned()),
        (QUERY_WORKSPACE_ID.to_owned(), workspace.to_owned()),
        (QUERY_FLEET.to_owned(), "Production".to_owned()),
    ]);
    let parsed = leases(&params).expect("every dimension is valid");
    assert_eq!(parsed.limit, 100);
    assert_eq!(
        parsed.starting_after.as_ref().map(Uuid7::as_str),
        Some(cursor)
    );
    assert_eq!(
        parsed.workspace.as_ref().map(Uuid7::as_str),
        Some(workspace)
    );
    assert_eq!(parsed.fleet.as_deref(), Some("Production"));

    for (key, value, detail) in [
        (QUERY_LIMIT, "0", DETAIL_BAD_LEASE_LIMIT),
        (QUERY_STARTING_AFTER, "foreign", DETAIL_BAD_LEASE_CURSOR),
        (QUERY_WORKSPACE_ID, "workspace", DETAIL_BAD_WORKSPACE),
        (QUERY_FLEET, "", DETAIL_BAD_FLEET),
    ] {
        assert_eq!(
            leases(&HashMap::from([(key.to_owned(), value.to_owned())])).err(),
            Some(detail)
        );
    }
}

#[test]
fn event_query_parses_sets_and_windows_and_refuses_partial_shapes() {
    let cursor = "1725000000000:0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
    let params = HashMap::from([
        (QUERY_LIMIT.to_owned(), "2".to_owned()),
        (QUERY_STARTING_AFTER.to_owned(), cursor.to_owned()),
        (
            QUERY_EVENT_TYPE.to_owned(),
            "runner_online,runner_offline".to_owned(),
        ),
        (QUERY_SINCE.to_owned(), "10".to_owned()),
        (QUERY_UNTIL.to_owned(), "20".to_owned()),
    ]);
    let parsed = events(&params).expect("the event query is valid");
    assert_eq!(parsed.limit, 2);
    assert_eq!(parsed.cursor.as_ref().map(format).as_deref(), Some(cursor));

    for (key, value, detail) in [
        (QUERY_PAGE, "2", DETAIL_RETIRED_EVENT_PAGE),
        (QUERY_PAGE_SIZE, "10", DETAIL_RETIRED_EVENT_PAGE),
        (QUERY_LIMIT, "0", DETAIL_BAD_EVENTS),
        (QUERY_STARTING_AFTER, "not-a-cursor", DETAIL_BAD_EVENTS),
        (QUERY_EVENT_TYPE, "", DETAIL_BAD_EVENTS),
        (QUERY_EVENT_TYPE, "runner_online,", DETAIL_BAD_EVENTS),
        (QUERY_EVENT_TYPE, "not_an_event", DETAIL_BAD_EVENTS),
        (QUERY_SINCE, "yesterday", DETAIL_BAD_EVENTS),
    ] {
        assert_eq!(
            events(&HashMap::from([(key.to_owned(), value.to_owned())])).err(),
            Some(detail)
        );
    }
    assert_eq!(
        events(&HashMap::from([
            (QUERY_SINCE.to_owned(), "21".to_owned()),
            (QUERY_UNTIL.to_owned(), "20".to_owned()),
        ]))
        .err(),
        Some(DETAIL_BAD_EVENTS)
    );
}

/// Each bounded filter refuses one past its bound and takes its bound.
#[test]
fn the_fleet_filter_and_the_event_type_set_are_bounded() {
    let fleet = |length: usize| {
        leases(&HashMap::from([(
            QUERY_FLEET.to_owned(),
            "f".repeat(length),
        )]))
        .err()
    };
    assert_eq!(fleet(MAX_FLEET_FILTER_LEN), None);
    assert_eq!(fleet(MAX_FLEET_FILTER_LEN + 1), Some(DETAIL_BAD_FLEET));

    let set = |count: usize| {
        let raw = vec!["runner_online"; count].join(",");
        events(&HashMap::from([(QUERY_EVENT_TYPE.to_owned(), raw)])).err()
    };
    assert_eq!(set(MAX_EVENT_TYPE_TOKENS), None);
    assert_eq!(set(MAX_EVENT_TYPE_TOKENS + 1), Some(DETAIL_BAD_EVENTS));
}

/// A blank `?limit=` is the default page on all three lists.
#[test]
fn a_blank_limit_is_the_default_page() {
    let blank = HashMap::from([(QUERY_LIMIT.to_owned(), String::new())]);
    let default = CEILING.default_rows();
    assert_eq!(page(&blank).map(|query| query.limit).ok(), Some(default));
    assert_eq!(leases(&blank).map(|query| query.limit).ok(), Some(default));
    assert_eq!(events(&blank).map(|query| query.limit).ok(), Some(default));
}
