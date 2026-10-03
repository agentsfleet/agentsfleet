#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::collections::BTreeSet;

use super::*;
use crate::stub::Stub;

/// The architecture document whose §"Tool catalog" table the catalog mirrors.
const ARCHITECTURE: &str = include_str!("../../../../../docs/architecture/runner_execution.md");

fn catalog() -> Catalog {
    Catalog::new(vec![
        Stub::boxed(&HTTP_REQUEST),
        Stub::boxed(&MEMORY_RECALL),
        Stub::boxed(&CALCULATOR),
        Stub::boxed(&FILE_READ),
    ])
}

fn names<'a>(selection: &'a Selection<'_>) -> Vec<&'a str> {
    selection.specs().map(|spec| spec.name).collect()
}

#[test]
fn test_catalog_offers_policy_tools() {
    let catalog = catalog();

    let selection = catalog.select(&["http_request", "memory_recall"]).unwrap();

    assert_eq!(names(&selection), ["http_request", "memory_recall"]);
    for spec in selection.specs() {
        assert_eq!(
            spec.description, spec.name,
            "each spec carries its own schema"
        );
        assert_eq!(spec.parameters["type"], "object");
    }
    assert!(
        selection.tool("calculator").is_none(),
        "nothing outside the policy"
    );
}

#[test]
fn a_tool_without_a_handler_refuses_the_selection() {
    let failure = catalog().select(&["http_request", "browser"]).unwrap_err();

    assert_eq!(failure.unhosted_tool(), Some("browser"));
    assert_eq!(
        failure.code(),
        afd_core::error_code::AGENTSFLEET_INVALID_CONFIG
    );
    assert!(failure.to_string().contains("browser"));
}

#[test]
fn an_unpublished_name_refuses_the_selection() {
    let failure = catalog().select(&["teleport"]).unwrap_err();

    assert_eq!(failure.unhosted_tool(), Some("teleport"));
}

#[test]
fn should_refuse_a_name_that_is_not_exactly_a_published_one() {
    let catalog = catalog();

    for name in [
        "",
        "HTTP_REQUEST",
        " http_request",
        "http_request\n",
        "http_requ\u{0435}st",
    ] {
        let failure = catalog.select(&[name]).unwrap_err();

        assert_eq!(failure.unhosted_tool(), Some(name), "{name:?}");
    }
}

#[test]
fn should_name_the_first_unhosted_tool_when_several_are() {
    let failure = catalog()
        .select(&["calculator", "browser", "shell"])
        .unwrap_err();

    assert_eq!(failure.unhosted_tool(), Some("browser"));
}

#[test]
fn a_provider_hosted_tool_needs_no_handler_and_is_no_function() {
    let catalog = catalog();

    let selection = catalog.select(&["web_search", "calculator"]).unwrap();

    assert!(selection.hosts("web_search"));
    assert_eq!(selection.hosted(), [&WEB_SEARCH]);
    assert_eq!(
        names(&selection),
        ["calculator"],
        "the provider sends its own spec"
    );
}

#[test]
fn a_name_the_policy_repeats_is_offered_once() {
    let catalog = catalog();

    let selection = catalog
        .select(&["calculator", "calculator", "web_search", "web_search"])
        .unwrap();

    assert_eq!(names(&selection), ["calculator"]);
    assert_eq!(selection.hosted().len(), 1);
}

#[test]
fn only_a_sandbox_side_tool_needs_a_sandbox() {
    let catalog = catalog();

    assert!(
        !catalog
            .select(&["http_request", "calculator"])
            .unwrap()
            .needs_sandbox()
    );
    assert!(!catalog.select(&["web_search"]).unwrap().needs_sandbox());
    assert!(!catalog.select::<&str>(&[]).unwrap().needs_sandbox());
    assert!(
        catalog
            .select(&["calculator", "file_read"])
            .unwrap()
            .needs_sandbox()
    );
}

#[test]
fn the_first_handler_for_a_tool_is_the_one_used() {
    let catalog = Catalog::new(vec![Stub::boxed(&CALCULATOR), Stub::boxed(&CALCULATOR)]);

    let selection = catalog.select(&["calculator"]).unwrap();

    assert_eq!(names(&selection), ["calculator"]);
}

#[test]
fn every_published_name_is_distinct() {
    let distinct: BTreeSet<&str> = PUBLISHED.iter().map(|entry| entry.name()).collect();

    assert_eq!(distinct.len(), PUBLISHED.len());
    assert!(
        PUBLISHED
            .iter()
            .all(|entry| published(entry.name()) == Some(*entry))
    );
}

#[test]
fn the_catalog_publishes_exactly_the_architecture_table() {
    let section = ARCHITECTURE
        .split("## Tool catalog")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .unwrap();
    let documented: BTreeSet<&str> = section
        .lines()
        .filter(|line| line.starts_with("| `"))
        .filter_map(|line| line.split('|').nth(1))
        .flat_map(|cell| cell.split('`').skip(1).step_by(2))
        .collect();
    let published: BTreeSet<&str> = PUBLISHED.iter().map(|entry| entry.name()).collect();

    assert_eq!(published, documented);
}
