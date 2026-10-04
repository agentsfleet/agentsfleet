//! What the model is offered: the selection's handlers as the provider's
//! function specs, and its hosted entries as tools the provider runs.
//!
//! The catalog and the providers each own their half; this is the one place
//! that reads both, so neither crate depends on the other.

use afr_providers::{Hosted, ToolSpec};
use afr_tools::catalog::WEB_SEARCH;
use afr_tools::{Entry, Selection};

/// The function specs `selection` offers, one per handler.
pub(crate) fn specs<'run>(selection: &Selection<'run>) -> Vec<ToolSpec<'run>> {
    selection
        .tools()
        .map(|tool| {
            let schema = tool.schema();
            ToolSpec {
                name: tool.name(),
                description: schema.description(),
                parameters: schema.parameters(),
            }
        })
        .collect()
}

/// The hosted tools `selection` offers, as the provider runs them. An entry
/// no provider runs is left out, and a call to it reaches the router.
pub(crate) fn hosted(selection: &Selection<'_>) -> Vec<Hosted> {
    selection
        .hosted()
        .iter()
        .copied()
        .filter_map(run_by)
        .collect()
}

/// The hosted tool `entry` is, when a provider runs it.
fn run_by(entry: &Entry) -> Option<Hosted> {
    (*entry == WEB_SEARCH).then_some(Hosted::WebSearch)
}

#[cfg(test)]
#[path = "offer/tests.rs"]
mod tests;
