//! `list_agents`: every child of the run and where it is.

use afr_tools::NoArguments;
use afr_tools::{ToolOutput, parsed};
use serde_json::Value;

use super::answer::json;
use crate::harness::Harness;

pub(super) fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    if let Err(refused) = parsed::<NoArguments>(arguments) {
        return refused;
    }
    json(&harness.shared.registry.list())
}
