#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;

use afr_egress::testing::RecordingTransport;
use afr_providers::Hosted;
use afr_tools::Catalog;
use afr_tools::catalog::{UPDATE_PLAN, WEB_SEARCH};

use super::{hosted, run_by, specs};

#[test]
fn should_offer_each_handler_under_its_own_schema_and_search_as_hosted() {
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let catalog = Catalog::hosted(Arc::new(transport));
    let selection = catalog
        .select(&[UPDATE_PLAN.name(), WEB_SEARCH.name()])
        .unwrap();

    let offered = specs(&selection);

    let schema = selection.tool(UPDATE_PLAN.name()).unwrap().schema();
    assert_ne!(
        schema.description(),
        UPDATE_PLAN.name(),
        "a real description"
    );
    assert_eq!(offered.len(), 1, "a hosted tool is no function spec");
    assert_eq!(offered[0].name, UPDATE_PLAN.name());
    assert_eq!(offered[0].description, schema.description());
    assert_eq!(offered[0].parameters, schema.parameters());
    assert_eq!(hosted(&selection), [Hosted::WebSearch]);
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
