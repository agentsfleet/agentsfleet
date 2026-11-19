//! What the model is offered: the selection's functions as one loop's tools,
//! and its hosted entries as tools the provider runs.
//!
//! The catalog and the providers each own their half; this is the one place
//! that reads both, so neither crate depends on the other.

use afr_tools::catalog::WEB_SEARCH;
use afr_tools::{Entry, Offered, Selection};
use guffbolt::Hosted;
use guffbolt::Tools;

use crate::nested::NestedTool;
use crate::router::{LoopTool, Routed};

/// The tools `selection` offers one loop, in the order its policy named
/// them: each handler as the router runs it, and each nested tool as the loop
/// runs it.
pub(crate) fn tools<'s, 'run>(selection: &'s Selection<'run>) -> Tools<LoopTool<'s, 'run>> {
    Tools::new(
        selection
            .offered()
            .map(|offered| -> Box<LoopTool<'s, 'run>> {
                match offered {
                    Offered::Handler(entry, handler) => {
                        Box::new(Routed::new(handler, entry.runtime()))
                    }
                    Offered::Nested(nested) => Box::new(NestedTool::new(nested)),
                }
            })
            .collect(),
    )
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
mod tests;
