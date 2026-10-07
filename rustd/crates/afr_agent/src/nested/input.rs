//! `send_input`: a message for a spawned child's next turn.

use afr_tools::nested::Input;
use afr_tools::{ToolOutput, parsed};
use serde_json::Value;

use super::answer::{Accepted, json, not_found};
use crate::harness::Harness;

pub(super) fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    let Input { child_id, message } = match parsed(arguments) {
        Ok(input) => input,
        Err(refused) => return refused,
    };
    match harness.shared.registry.send(child_id, message) {
        Some(accepted) => json(&Accepted { accepted }),
        None => not_found(child_id),
    }
}
