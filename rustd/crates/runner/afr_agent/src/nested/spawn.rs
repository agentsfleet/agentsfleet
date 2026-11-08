//! `spawn`: a child started alongside, named by id.

use afr_tools::nested::Task;
use afr_tools::{ToolOutput, parsed};
use serde_json::Value;

use super::answer::{Spawned, json};
use super::start::start;
use crate::harness::Harness;

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
