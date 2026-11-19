//! `send_input`: a message for a spawned child's next turn.

use afr_tools::nested::Input;
use guffbolt::ToolOutput;
use guffbolt::agents::{Accepted, not_found};
use guffbolt::tools::parsed;
use serde_json::Value;

use super::answer::json;
use crate::harness::Harness;

/// What `send_input` does, as the model reads it.
pub(super) const DESCRIPTION: &str = "Send a running child a message it reads on its next \
        turn. Answers whether it was accepted; a child that has ended accepts none.";

pub(super) fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    let Input { child_id, message } = match parsed(arguments) {
        Ok(input) => input,
        Err(refused) => return refused,
    };
    harness.shared.registry.send(child_id, message).map_or_else(
        || not_found(child_id),
        |accepted| json(&Accepted { accepted }),
    )
}
