#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::collections::BTreeSet;

use guffbolt::agents::TOOLS;

use super::*;
use crate::stub::Stub;

fn catalog() -> Catalog {
    Catalog::new(vec![
        Stub::boxed(&HTTP_REQUEST),
        Stub::boxed(&MEMORY_RECALL),
        Stub::boxed(&UPDATE_PLAN),
        Stub::boxed(&FILE_READ),
    ])
}

fn names(selection: &Selection<'_>) -> Vec<&'static str> {
    selection.offered().map(Offered::name).collect()
}

#[test]
fn test_catalog_offers_policy_tools() {
    let catalog = catalog();

    let selection = catalog.select(&["http_request", "memory_recall"]).unwrap();

    assert_eq!(names(&selection), ["http_request", "memory_recall"]);
    for name in names(&selection) {
        let spec = selection.tool(name).unwrap().spec();
        assert_eq!(spec.name, name);
        assert_eq!(
            spec.description, name,
            "each handler carries its own schema"
        );
        assert_eq!(spec.parameters["type"], "object");
    }
    assert!(
        selection.tool("update_plan").is_none(),
        "nothing outside the policy"
    );
}

/// A published tool no run builds, such as a fleet tool, needs its handler.
#[test]
fn a_tool_without_a_handler_refuses_the_selection() {
    let failure = catalog()
        .select(&["http_request", "fleet_steer"])
        .unwrap_err();

    assert_eq!(failure.unhosted_tool(), Some("fleet_steer"));
    assert_eq!(
        failure.code(),
        af_core::error_code::AGENTSFLEET_INVALID_CONFIG
    );
    assert!(failure.to_string().contains("fleet_steer"));
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
        .select(&["update_plan", "fleet_steer", "fleet_activity"])
        .unwrap_err();

    assert_eq!(failure.unhosted_tool(), Some("fleet_steer"));
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

/// The schedule and message tools are built by each run over its verbs, so a
/// run naming any of them runs rather than being refused; none needs a
/// sandbox.
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

/// `guffbolt`'s subagent tools are offered under `guffbolt`'s own names,
/// which this catalog publishes nowhere, and run beside the loop.
#[test]
fn guffbolts_subagent_tools_are_offered_by_their_own_names() {
    let catalog = Catalog::new(Vec::new());
    let selection = catalog.select(&TOOLS).unwrap();

    assert_eq!(names(&selection), TOOLS);
    for offered in selection.offered() {
        assert!(matches!(offered, Offered::Built(_)), "{offered:?}");
        assert_eq!(offered.runtime(), Runtime::Supervisor);
        assert_eq!(published(offered.name()), None);
    }
    assert!(!selection.needs_sandbox());
}

/// Every published entry a handler serves is hosted, so a policy naming any
/// of them is admitted; one left out of `hosted` would refuse every run
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

/// The two fleet tools are offered only to a run whose
/// policy names them, and both are hosted here.
#[test]
fn fleet_tools_are_offered_only_when_declared() {
    let (transport, _sent) = crate::testing::replying(200, "");
    let catalog = Catalog::hosted(transport);

    let without = catalog.select(&["message"]).unwrap();
    assert!(!without.offers("fleet_steer"));
    assert!(!without.offers("fleet_activity"));

    let with = catalog
        .select(&["message", "fleet_steer", "fleet_activity"])
        .unwrap();
    assert!(with.offers("fleet_steer"));
    assert!(with.offers("fleet_activity"));
    assert_eq!(PUBLISHED.len(), 35);
}

/// A tool a run builds over its own backends is offered with no handler, and
/// a handler the catalog hosts under its name is offered in its place.
#[test]
fn a_backed_tool_is_offered_without_a_handler_unless_one_is_hosted() {
    let (empty, stubbed) = (Catalog::new(Vec::new()), catalog());
    let built = empty
        .select(&[NOTIFY.name(), MEMORY_RECALL.name()])
        .unwrap();
    let hosted = stubbed.select(&[MEMORY_RECALL.name()]).unwrap();

    let built: Vec<Offered<'_>> = built.offered().collect();
    assert!(
        matches!(built.as_slice(), [Offered::Backed(first), Offered::Backed(second)]
            if **first == NOTIFY && **second == MEMORY_RECALL),
        "{built:?}"
    );
    let hosted: Vec<Offered<'_>> = hosted.offered().collect();
    assert!(
        matches!(hosted.as_slice(), [Offered::Handler(entry, _)] if **entry == MEMORY_RECALL),
        "{hosted:?}"
    );
}

/// A run notifies through `notify`; the vendor's name is no tool a policy
/// may name.
#[test]
fn notify_is_published_and_pushover_is_not() {
    assert_eq!(published("notify"), Some(&NOTIFY));
    assert_eq!(published("pushover"), None);
    let refused = Catalog::new(Vec::new()).select(&["pushover"]).unwrap_err();
    assert_eq!(refused.unhosted_tool(), Some("pushover"));
}

/// Every backed entry is published, runs beside the loop or in the sandbox,
/// and is built by a run that names it.
#[test]
fn every_backed_entry_is_published_and_built_by_its_run() {
    let run = crate::run::Run::default();
    let slot = crate::testing::no_sandbox();
    let backends = crate::Backends::new(&run, &slot, None);
    let names: Vec<&str> = BACKED.iter().map(|entry| entry.name()).collect();
    let tools = backends.tools(&names).unwrap();

    for entry in BACKED {
        assert_eq!(published(entry.name()), Some(entry));
        assert_ne!(entry.runtime(), Runtime::Provider, "{}", entry.name());
        assert!(
            tools.tool(entry.name()).is_some(),
            "{} is built",
            entry.name()
        );
    }
}
