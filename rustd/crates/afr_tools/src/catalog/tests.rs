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
        Stub::boxed(&UPDATE_PLAN),
        Stub::boxed(&FILE_READ),
    ])
}

fn names<'a>(selection: &'a Selection<'_>) -> Vec<&'a str> {
    selection.tools().map(crate::Tool::name).collect()
}

#[test]
fn test_catalog_offers_policy_tools() {
    let catalog = catalog();

    let selection = catalog.select(&["http_request", "memory_recall"]).unwrap();

    assert_eq!(names(&selection), ["http_request", "memory_recall"]);
    for tool in selection.tools() {
        let schema = tool.schema();
        assert_eq!(
            schema.description(),
            tool.name(),
            "each handler carries its own schema"
        );
        assert_eq!(schema.parameters()["type"], "object");
    }
    assert!(
        selection.tool("update_plan").is_none(),
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
        .select(&["update_plan", "browser", "shell"])
        .unwrap_err();

    assert_eq!(failure.unhosted_tool(), Some("browser"));
}

#[test]
fn a_provider_hosted_tool_needs_no_handler_and_is_no_function() {
    let catalog = catalog();

    let selection = catalog.select(&["web_search", "update_plan"]).unwrap();

    assert!(selection.hosts("web_search"));
    assert_eq!(selection.hosted(), [&WEB_SEARCH]);
    assert_eq!(
        names(&selection),
        ["update_plan"],
        "the provider sends its own spec"
    );
}

#[test]
fn a_name_the_policy_repeats_is_offered_once() {
    let catalog = catalog();

    let selection = catalog
        .select(&["update_plan", "update_plan", "web_search", "web_search"])
        .unwrap();

    assert_eq!(names(&selection), ["update_plan"]);
    assert_eq!(selection.hosted().len(), 1);
}

#[test]
fn only_a_sandbox_side_tool_needs_a_sandbox() {
    let catalog = catalog();

    assert!(
        !catalog
            .select(&["http_request", "update_plan"])
            .unwrap()
            .needs_sandbox()
    );
    assert!(!catalog.select(&["web_search"]).unwrap().needs_sandbox());
    assert!(!catalog.select::<&str>(&[]).unwrap().needs_sandbox());
    assert!(
        catalog
            .select(&["update_plan", "file_read"])
            .unwrap()
            .needs_sandbox()
    );
}

#[test]
fn the_first_handler_for_a_tool_is_the_one_used() {
    let catalog = Catalog::new(vec![Stub::boxed(&UPDATE_PLAN), Stub::boxed(&UPDATE_PLAN)]);

    let selection = catalog.select(&["update_plan"]).unwrap();

    assert_eq!(names(&selection), ["update_plan"]);
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

/// The name a test-built entry carries; no published tool has it.
const UNPUBLISHED: &str = "an_unpublished_tool";

/// Every published entry is built in a `const`, so this is the one build the
/// runtime sees: an entry reads back the name and runtime it was given.
#[test]
fn an_entry_reads_back_the_name_and_runtime_it_was_built_with() {
    let entry = Entry::new(UNPUBLISHED, Runtime::Sandbox);

    assert_eq!(entry.name(), UNPUBLISHED);
    assert_eq!(entry.runtime(), Runtime::Sandbox);
}

/// The schedule and message tools are hosted, so a lease naming any of them
/// runs rather than being refused; none needs a sandbox.
#[test]
fn the_hosted_catalog_offers_every_schedule_and_message_tool() {
    let (transport, _sent) = crate::testing::replying(200, "");
    let catalog = Catalog::hosted(transport);
    let eight = [
        MESSAGE.name(),
        SCHEDULE.name(),
        CRON_ADD.name(),
        CRON_LIST.name(),
        CRON_REMOVE.name(),
        CRON_UPDATE.name(),
        CRON_RUN.name(),
        CRON_RUNS.name(),
    ];
    let selection = catalog.select(&eight).unwrap();
    assert_eq!(names(&selection), eight);
    assert!(!selection.needs_sandbox());
}

/// A child's selection is its parent's narrowed to the names it asked for,
/// hosted entries included, and the first name the parent lacks refuses it.
#[test]
fn a_selection_narrows_to_names_it_offers_and_refuses_one_it_does_not() {
    let catalog = catalog();
    let parent = catalog
        .select(&[FILE_READ.name(), WEB_SEARCH.name(), UPDATE_PLAN.name()])
        .unwrap();

    let narrowed = parent
        .narrowed(&[WEB_SEARCH.name(), FILE_READ.name(), FILE_READ.name()])
        .unwrap();
    let asked = [FILE_READ.name(), SHELL.name(), HTTP_REQUEST.name()];
    let refused = parent.narrowed(&asked);

    assert_eq!(names(&narrowed), [FILE_READ.name()]);
    assert_eq!(narrowed.hosted(), [&WEB_SEARCH]);
    assert_eq!(refused.err(), Some(SHELL.name()), "the first name not held");
}

/// Dropping entries leaves the rest, handlers and hosted alike.
#[test]
fn a_selection_without_entries_keeps_the_rest() {
    let catalog = catalog();
    let parent = catalog
        .select(&[FILE_READ.name(), WEB_SEARCH.name(), UPDATE_PLAN.name()])
        .unwrap();

    let without = parent.without(&[&UPDATE_PLAN, &WEB_SEARCH]);

    assert_eq!(names(&without), [FILE_READ.name()]);
    assert!(without.hosted().is_empty());
    assert_eq!(names(&parent).len(), 2, "the parent is untouched");
}

/// Every published entry a handler serves is hosted, so a policy naming any
/// of them is admitted; one left out of `hosted` would refuse every lease
/// naming it at admission, which is how a tool goes dark.
#[test]
fn every_published_handler_entry_is_hosted() {
    let (transport, _sent) = crate::testing::replying(200, "");
    let catalog = Catalog::hosted(transport);
    let served: Vec<&str> = PUBLISHED
        .iter()
        .filter(|entry| entry.runtime() != Runtime::Provider)
        .map(|entry| entry.name())
        .collect();

    let unhosted: Vec<&str> = served
        .iter()
        .copied()
        .filter(|name| catalog.select(&[*name]).is_err())
        .collect();

    assert_eq!(
        served.len(),
        PUBLISHED.len() - 1,
        "one entry is the provider's"
    );
    assert!(
        unhosted.is_empty(),
        "published but not hosted: {unhosted:?}"
    );
}
