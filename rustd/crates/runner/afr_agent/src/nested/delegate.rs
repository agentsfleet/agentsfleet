//! `delegate`: a child run to its answer, which is the call's output.

use afr_tools::nested::Task;
use afr_tools::parsed;
use guffbolt::ToolOutput;
use serde_json::Value;

use super::start::start;
use crate::harness::Harness;

/// What `delegate` does, as the model reads it.
pub(super) const DESCRIPTION: &str = "Hand a task to a child agent that shares this run's \
        sandbox, files, memory and budget, and wait for its answer. The child starts from the \
        task alone and may hold a subset of your tools.";

pub(super) async fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    let task: Task = match parsed(arguments) {
        Ok(task) => task,
        Err(refused) => return refused,
    };
    let id = match start(harness, task) {
        Ok(id) => id,
        Err(refused) => return refused,
    };
    harness.shared.registry.delegated(id).await
}
