//! `spawn`: a child started alongside, named by id.

use afr_tools::nested::Task;
use afr_tools::parsed;
use guffbolt::ToolOutput;
use guffbolt::agents::Spawned;
use serde_json::Value;

use super::answer::json;
use super::start::start;
use crate::harness::Harness;

/// What `spawn` does, as the model reads it.
pub(super) const DESCRIPTION: &str = "Start a child agent on a task and keep working while it \
        runs; it shares this run's sandbox, files, memory and budget. Answers with a child_id \
        for wait_agent, send_input and interrupt_agent.";

pub(super) fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    let task: Task = match parsed(arguments) {
        Ok(task) => task,
        Err(refused) => return refused,
    };
    match start(harness, task) {
        Ok(child_id) => json(&Spawned { child_id }),
        Err(refused) => refused,
    }
}
