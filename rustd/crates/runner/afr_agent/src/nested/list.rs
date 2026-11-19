//! `list_agents`: every child of the run and where it is.

use afr_tools::{NoArguments, parsed};
use guffbolt::ToolOutput;
use serde_json::Value;

use super::answer::json;
use crate::harness::Harness;

/// What `list_agents` does, as the model reads it.
pub(super) const DESCRIPTION: &str =
    "List every child agent of this run: its id, status, depth and how many calls it made.";

pub(super) fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    if let Err(refused) = parsed::<NoArguments>(arguments) {
        return refused;
    }
    json(&harness.shared.registry.list())
}
