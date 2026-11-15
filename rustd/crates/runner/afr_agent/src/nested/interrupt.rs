//! `interrupt_agent`: one spawned child ended where it is.

use afr_tools::nested::Named;
use afr_tools::{ToolOutput, parsed};
use serde_json::Value;

use super::answer::{State, json, not_found};
use crate::harness::Harness;

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
