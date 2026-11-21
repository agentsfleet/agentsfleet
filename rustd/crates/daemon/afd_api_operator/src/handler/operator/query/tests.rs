//! What the operator lists read off a query string, and what each refuses.

#![expect(clippy::expect_used, reason = "tests inspect canonical query fixtures")]

use super::*;

/// The list page when the deployment names none.
const CEILING: Ceiling = af_core::paging::CEILING;

fn page(params: &HashMap<String, String>) -> Result<PageQuery, Bad> {
    super::page(params, CEILING)
}

fn runs(params: &HashMap<String, String>) -> Result<RunQuery, Bad> {
    super::runs(params, CEILING)
}

fn events(params: &HashMap<String, String>) -> Result<EventQuery, Bad> {
    super::events(params, CEILING)
}

#[test]
fn page_query_accepts_defaults_and_composite_cursors() {
    let empty = HashMap::new();
    let defaults = page(&empty).expect("defaults are valid");
    assert_eq!(defaults.limit, CEILING.default_rows());
    assert!(defaults.cursor.is_none());

    let mut params = HashMap::new();
    params.insert(QUERY_LIMIT.to_owned(), "100".to_owned());
    let cursor = "2024-08-30T06:40:00Z:0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
    params.insert(QUERY_STARTING_AFTER.to_owned(), cursor.to_owned());
    let parsed = page(&params).expect("the boundary is canonical");
    assert_eq!(parsed.limit, 100);
    assert_eq!(
        format(parsed.cursor.as_ref().expect("cursor exists")),
        cursor
    );
}

#[test]
fn page_query_refuses_out_of_range_inputs() {
    for (key, value, detail) in [
        (QUERY_LIMIT, "0", Bad::Page),
        (QUERY_LIMIT, "101", Bad::Page),
        (QUERY_STARTING_AFTER, "not-a-cursor", Bad::Page),
    ] {
        let params = HashMap::from([(key.to_owned(), value.to_owned())]);
        assert_eq!(page(&params).err(), Some(detail));
    }
}

#[test]
fn run_query_parses_independent_filters_and_refuses_each_bad_dimension() {
    let cursor = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
    let workspace = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a7d";
    let params = HashMap::from([
        (QUERY_LIMIT.to_owned(), "100".to_owned()),
        (QUERY_STARTING_AFTER.to_owned(), cursor.to_owned()),
        (QUERY_WORKSPACE_ID.to_owned(), workspace.to_owned()),
        (QUERY_FLEET.to_owned(), "Production".to_owned()),
    ]);
    let parsed = runs(&params).expect("every dimension is valid");
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
        (QUERY_LIMIT, "0", Bad::RunLimit),
        (QUERY_STARTING_AFTER, "foreign", Bad::RunCursor),
        (QUERY_WORKSPACE_ID, "workspace", Bad::Workspace),
        (QUERY_FLEET, "", Bad::Fleet),
    ] {
        assert_eq!(
            runs(&HashMap::from([(key.to_owned(), value.to_owned())])).err(),
            Some(detail)
        );
    }
}

#[test]
fn event_query_parses_sets_and_windows_and_refuses_partial_shapes() {
    let cursor = "2024-08-30T06:40:00Z:0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
    let params = HashMap::from([
        (QUERY_LIMIT.to_owned(), "2".to_owned()),
        (QUERY_STARTING_AFTER.to_owned(), cursor.to_owned()),
        (
            QUERY_EVENT_TYPE.to_owned(),
            "runner_online,runner_offline".to_owned(),
        ),
        (QUERY_SINCE.to_owned(), "2024-08-30T06:40:10Z".to_owned()),
        (QUERY_UNTIL.to_owned(), "2024-08-30T06:40:20.5Z".to_owned()),
    ]);
    let parsed = events(&params).expect("the event query is valid");
    assert_eq!(parsed.limit, 2);
    assert_eq!(parsed.cursor.as_ref().map(format).as_deref(), Some(cursor));

    for (key, value, detail) in [
        (QUERY_LIMIT, "0", Bad::Events),
        (QUERY_STARTING_AFTER, "not-a-cursor", Bad::Events),
        (QUERY_EVENT_TYPE, "", Bad::Events),
        (QUERY_EVENT_TYPE, "runner_online,", Bad::Events),
        (QUERY_EVENT_TYPE, "not_an_event", Bad::Events),
        (QUERY_SINCE, "yesterday", Bad::Events),
        (QUERY_SINCE, "1725000010000", Bad::Events),
        (QUERY_UNTIL, "2024-08-30T06:40:20.0001Z", Bad::Events),
    ] {
        assert_eq!(
            events(&HashMap::from([(key.to_owned(), value.to_owned())])).err(),
            Some(detail)
        );
    }
    assert_eq!(
        events(&HashMap::from([
            (QUERY_SINCE.to_owned(), "2024-08-30T06:40:21Z".to_owned()),
            (QUERY_UNTIL.to_owned(), "2024-08-30T06:40:20Z".to_owned()),
        ]))
        .err(),
        Some(Bad::Events)
    );
}

/// Each bounded filter refuses one past its bound and takes its bound.
#[test]
fn the_fleet_filter_and_the_event_type_set_are_bounded() {
    let fleet = |length: usize| {
        runs(&HashMap::from([(
            QUERY_FLEET.to_owned(),
            "f".repeat(length),
        )]))
        .err()
    };
    assert_eq!(fleet(MAX_FLEET_FILTER_LEN), None);
    assert_eq!(fleet(MAX_FLEET_FILTER_LEN + 1), Some(Bad::Fleet));

    let set = |count: usize| {
        let raw = vec!["runner_online"; count].join(",");
        events(&HashMap::from([(QUERY_EVENT_TYPE.to_owned(), raw)])).err()
    };
    // Every type the enum holds, and more tokens than that, are one filter.
    assert_eq!(set(20), None);
}

/// A blank `?limit=` is the default page on all three lists.
#[test]
fn a_blank_limit_is_the_default_page() {
    let blank = HashMap::from([(QUERY_LIMIT.to_owned(), String::new())]);
    let default = CEILING.default_rows();
    assert_eq!(page(&blank).map(|query| query.limit).ok(), Some(default));
    assert_eq!(runs(&blank).map(|query| query.limit).ok(), Some(default));
    assert_eq!(events(&blank).map(|query| query.limit).ok(), Some(default));
}

/// Each token reads as the variant its serde spelling names, and a word no
/// variant spells refuses the whole set rather than being dropped from it.
#[test]
fn an_event_type_token_reads_by_its_stored_spelling_and_an_unknown_one_refuses() {
    let set = |raw: &str| {
        events(&HashMap::from([(
            QUERY_EVENT_TYPE.to_owned(),
            raw.to_owned(),
        )]))
    };
    let read = set("runner_online,run_released").expect("two stored spellings");
    let expected = RunnerHistoryFilter::new(
        vec![RunnerEventType::RunnerOnline, RunnerEventType::RunReleased],
        None,
        None,
    );
    assert_eq!(Some(read.filter), expected);
    for unknown in [
        "RunnerOnline",
        "runner_online,not_an_event",
        "runner-online",
    ] {
        assert_eq!(set(unknown).err(), Some(Bad::Events), "{unknown}");
    }
}

/// The boundary is a timestamp and a version-7 id: a text-form cursor, an
/// empty id, an id that is not one and a boundary that is not digits are each
/// the page sentence.
#[test]
fn a_cursor_that_is_not_a_timestamp_and_a_uuid7_is_refused() {
    let text_form = Cursor::Text {
        value: "1725000000000".to_owned(),
        id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010".to_owned(),
    }
    .to_string();
    for refused in [
        text_form.as_str(),
        "1725000000000:",
        "1725000000000:not-a-uuid",
        "not-digits:0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010",
    ] {
        let params = HashMap::from([(QUERY_STARTING_AFTER.to_owned(), refused.to_owned())]);
        assert_eq!(page(&params).err(), Some(Bad::Page), "{refused}");
    }
}

/// `[api]`'s list page bounds every operator list, and each sentence names it.
#[test]
fn the_configured_list_page_bounds_and_names_every_list() {
    let configured = Ceiling::new(7, 3);
    let blank = HashMap::new();
    assert_eq!(
        super::page(&blank, configured).map(|q| q.limit).ok(),
        Some(3)
    );
    let over = HashMap::from([(QUERY_LIMIT.to_owned(), "8".to_owned())]);
    assert_eq!(super::runs(&over, configured).err(), Some(Bad::RunLimit));
    assert!(Bad::Page.sentence(configured).contains("between 1 and 7"));
    assert!(Bad::Events.sentence(configured).contains("between 1 and 7"));
}
