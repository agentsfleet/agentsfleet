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
    match harness.shared.registry.interrupt(child_id) {
        Some(status) => json(&State::named(status)),
        None => not_found(child_id),
    }
}
