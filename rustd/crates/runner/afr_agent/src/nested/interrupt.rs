//! `interrupt_agent`: one spawned child ended where it is.

use afr_tools::nested::Named;
use afr_tools::parsed;
use guffbolt::ToolOutput;
use guffbolt::agents::{State, not_found};
use serde_json::Value;

use super::answer::json;
use crate::harness::Harness;

/// What `interrupt_agent` does, as the model reads it.
pub(super) const DESCRIPTION: &str =
    "Stop a spawned child where it is. Its open call ends interrupted, and so does it.";

pub(super) fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    let Named { child_id } = match parsed(arguments) {
        Ok(named) => named,
        Err(refused) => return refused,
    };
    harness
        .shared
        .registry
        .interrupt(child_id)
        .map_or_else(|| not_found(child_id), |status| json(&State::named(status)))
}
