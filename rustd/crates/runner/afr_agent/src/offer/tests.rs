#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;

use af_egress::testing::RecordingTransport;
use afr_tools::catalog::{DELEGATE, MEMORY_RECALL, UPDATE_PLAN, WEB_SEARCH};
use afr_tools::{Backends, Catalog, Run};
use guffbolt::Hosted;
use guffbolt::ToolSpec;

use super::{hosted, run_by, tools};

#[test]
fn should_offer_each_handler_under_its_own_schema_and_search_as_hosted() {
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let catalog = Catalog::hosted(Arc::new(transport));
    let selection = catalog
        .select(&[UPDATE_PLAN.name(), WEB_SEARCH.name()])
        .unwrap();
    let run = Run::default();
    let backends = Backends::new(&run, None);
    let backed = backends.tools::<&str>(&[]).unwrap();

    let offered = tools(&selection, &backed);

    let spec = selection.tool(UPDATE_PLAN.name()).unwrap().spec();
    assert_ne!(spec.description, UPDATE_PLAN.name(), "a real description");
    assert_eq!(
        offered.specs().collect::<Vec<ToolSpec<'_>>>(),
        [spec],
        "a hosted tool is no function spec"
    );
    assert_eq!(hosted(&selection), [Hosted::WebSearch]);
}

/// A nested tool is offered where its policy names it, under the schema the
/// loop runs it by, and no catalog handler stands behind it.
#[test]
fn should_offer_a_nested_tool_in_the_order_its_policy_names_it() {
    let catalog = Catalog::new(Vec::new());
    let selection = catalog.select(&[DELEGATE.name()]).unwrap();
    let run = Run::default();
    let backends = Backends::new(&run, None);
    let backed = backends.tools::<&str>(&[]).unwrap();

    let offered = tools(&selection, &backed);

    let names: Vec<&str> = offered.names().collect();
    assert_eq!(names, [DELEGATE.name()]);
    let spec = offered.specs().next().unwrap();
    assert!(spec.description.starts_with("Hand a task to a child agent"));
    assert_eq!(spec.parameters["required"], serde_json::json!(["task"]));
}

/// A tool the run built over its backends is offered where its policy names
/// it, under the schema it was built with; one the run did not build is not.
#[test]
fn should_offer_a_tool_the_run_built_and_pass_over_one_it_did_not() {
    let catalog = Catalog::new(Vec::new());
    let selection = catalog
        .select(&[MEMORY_RECALL.name(), UPDATE_PLAN.name()])
        .unwrap_err();
    assert_eq!(selection.unhosted_tool(), Some(UPDATE_PLAN.name()));
    let selection = catalog.select(&[MEMORY_RECALL.name()]).unwrap();
    let run = Run::default();
    let backends = Backends::new(&run, None);
    let built = backends.tools(&[MEMORY_RECALL.name()]).unwrap();
    let none = backends.tools::<&str>(&[]).unwrap();

    let offered = tools(&selection, &built);
    let unbuilt = tools(&selection, &none);

    let names: Vec<&str> = offered.names().collect();
    assert_eq!(names, [MEMORY_RECALL.name()]);
    let spec = offered.specs().next().unwrap();
    assert_eq!(spec.parameters["required"], serde_json::json!(["query"]));
    assert_eq!(unbuilt.names().count(), 0);
}

// The provider names its search as the catalog does, or the router would
// not know the model's call to it.
#[test]
fn should_offer_web_search_under_the_catalogs_name() {
    assert_eq!(run_by(&WEB_SEARCH), Some(Hosted::WebSearch));
    assert_eq!(Hosted::WebSearch.name(), WEB_SEARCH.name());
}

#[test]
fn should_host_nothing_the_catalog_runs_itself() {
    assert_eq!(run_by(&UPDATE_PLAN), None);
}
