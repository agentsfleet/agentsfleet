//! The architecture page's route tables and the route table the router serves
//! describe one surface.
//!
//! `docs/architecture/api_resources.md` is where a client author reads the
//! workspace-chat cutover: seventeen new person routes, seven changed, two
//! removed with no alias, and four authoring machine routes. `route_inventory`
//! pins the roster from the spec's own list and `openapi_coverage` pins the
//! published document to the router; neither reads the page. A route renamed
//! in code and not on the page, or a removed route that kept a handler, passes
//! both. This reads the page's tables and holds the router to every row.
//!
//! The expected counts are written out rather than read from the headings, so
//! a table that lost a row and a heading edited to match still fails here.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unreadable page should fail the test loudly"
)]

use std::collections::BTreeSet;

use afd_api::Route;

/// The page, from this crate's manifest directory.
const PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/architecture/api_resources.md"
);

const NEW_PERSON: &str = "### New person routes: ";
const CHANGED_PERSON: &str = "### Changed person routes: ";
const REMOVED_PERSON: &str = "### Removed person routes: ";
const MACHINE: &str = "Four new machine routes under ";

/// The cutover's shape, as the spec's goal states it.
const NEW_ROUTES: usize = 17;
const CHANGED_ROUTES: usize = 7;
const REMOVED_ROUTES: usize = 2;
const MACHINE_ROUTES: usize = 4;

/// The page's path prefixes, longest-lived first: `D`, `F` and `G` all sit
/// under `W`.
const PREFIXES: &[(&str, &str)] = &[
    ("W", "/v1/workspaces/{workspace_id}"),
    ("D", "W/drafts/{draft_id}"),
    ("F", "W/fleets/{fleet_id}"),
    ("G", "W/fleet-libraries/{library_id}"),
];

/// One operation: a path template and its method.
type Operation = (String, String);

fn served() -> BTreeSet<Operation> {
    Route::all()
        .flat_map(|route| {
            let template = route.meta().template;
            route
                .verbs()
                .iter()
                .map(move |verb| (template.to_owned(), verb.method().to_string()))
        })
        .collect()
}

fn page() -> String {
    std::fs::read_to_string(PAGE).expect("the architecture page is readable")
}

/// `W/models` → `/v1/workspaces/{workspace_id}/models`, expanding until no
/// prefix is left.
fn expand(resource: &str) -> String {
    let (head, rest) = resource.split_once('/').unwrap_or((resource, ""));
    let Some((_, path)) = PREFIXES.iter().find(|(prefix, _)| *prefix == head) else {
        return resource.to_owned();
    };
    let expanded = expand(path);
    if rest.is_empty() {
        expanded
    } else {
        format!("{expanded}/{rest}")
    }
}

/// The table rows under the line starting with `marker`, up to the first line
/// that is not a table row.
fn rows<'a>(page: &'a str, marker: &str) -> Vec<Vec<&'a str>> {
    page.lines()
        .skip_while(|line| !line.starts_with(marker))
        .skip(1)
        .skip_while(|line| !line.starts_with('|'))
        .take_while(|line| line.starts_with('|'))
        .map(|line| line.split('|').map(str::trim).collect::<Vec<_>>())
        .filter(|cells| cells.get(1).is_some_and(|first| !first.starts_with("---")))
        .skip(1)
        .collect()
}

/// A person table's operations: `| N1 | GET W/models | … |`.
fn person(page: &str, marker: &str) -> Vec<Operation> {
    rows(page, marker)
        .iter()
        .map(|cells| {
            let cell = cells.get(2).expect("a method and resource cell");
            let (method, resource) = cell.split_once(' ').expect("`METHOD resource`");
            (expand(resource.trim()), method.to_owned())
        })
        .collect()
}

/// The machine table's operations: ``| POST `/renew` | … |`` under the base
/// path the marker line names.
fn machine(page: &str) -> Vec<Operation> {
    let line = page
        .lines()
        .find(|line| line.starts_with(MACHINE))
        .expect("the machine routes are introduced");
    let base = line
        .split('`')
        .nth(1)
        .expect("the base path is in backticks");
    rows(page, MACHINE)
        .iter()
        .map(|cells| {
            let cell = cells.get(1).expect("a method and suffix cell");
            let (method, suffix) = cell.split_once(' ').expect("`METHOD suffix`");
            (
                format!("{base}{}", suffix.trim_matches('`')),
                method.to_owned(),
            )
        })
        .collect()
}

/// The count a heading declares: `### New person routes: 17` → 17.
fn declared(page: &str, marker: &str) -> usize {
    page.lines()
        .find_map(|line| line.strip_prefix(marker))
        .and_then(|count| count.trim().parse().ok())
        .expect("the heading declares a count")
}

fn missing(operations: &[Operation], served: &BTreeSet<Operation>) -> Vec<String> {
    operations
        .iter()
        .filter(|operation| !served.contains(*operation))
        .map(|(path, method)| format!("{method} {path}"))
        .collect()
}

#[test]
fn every_route_the_architecture_tables_name_is_served() {
    let page = page();
    let served = served();
    for (marker, expected) in [
        (NEW_PERSON, NEW_ROUTES),
        (CHANGED_PERSON, CHANGED_ROUTES),
        (REMOVED_PERSON, REMOVED_ROUTES),
    ] {
        let table = person(&page, marker);
        assert_eq!(table.len(), expected, "{marker}rows: {table:?}");
        assert_eq!(declared(&page, marker), expected, "{marker}heading");
    }
    let machine = machine(&page);
    assert_eq!(machine.len(), MACHINE_ROUTES, "machine rows: {machine:?}");

    let mut owed = person(&page, NEW_PERSON);
    owed.extend(person(&page, CHANGED_PERSON));
    owed.extend(machine);
    assert_eq!(
        missing(&owed, &served),
        Vec::<String>::new(),
        "every route the page names is served"
    );
}

#[test]
fn a_removed_route_is_served_under_no_method() {
    let page = page();
    let served = served();
    let removed = person(&page, REMOVED_PERSON);
    assert_eq!(removed.len(), REMOVED_ROUTES);
    for (path, method) in &removed {
        let aliases: Vec<_> = served
            .iter()
            .filter(|(template, _)| template == path)
            .collect();
        assert!(
            aliases.is_empty(),
            "{method} {path} was removed with no alias, yet the router serves {aliases:?}"
        );
    }
}

#[test]
fn a_prefix_expands_through_the_workspace() {
    assert_eq!(expand("W/models"), "/v1/workspaces/{workspace_id}/models");
    assert_eq!(
        expand("G/revisions/{revision_id}"),
        "/v1/workspaces/{workspace_id}/fleet-libraries/{library_id}/revisions/{revision_id}"
    );
    assert_eq!(
        expand("D"),
        "/v1/workspaces/{workspace_id}/drafts/{draft_id}"
    );
    assert_eq!(
        expand("/v1/models"),
        "/v1/models",
        "a full path is left alone"
    );
}

#[test]
fn a_table_row_the_router_lacks_is_reported_by_method_and_path() {
    let served = served();
    let invented = vec![(expand("W/library-entries"), "GET".to_owned())];
    assert_eq!(
        missing(&invented, &served),
        ["GET /v1/workspaces/{workspace_id}/library-entries"],
        "the removed path is exactly what a stale table row would name"
    );
}
