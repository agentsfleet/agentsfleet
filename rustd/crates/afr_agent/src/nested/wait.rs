//! `wait_agent`: a spawned child's end, or that it still runs.

use std::time::Duration;

use afr_tools::nested::Wait;
use afr_tools::{ToolOutput, parsed};
use serde_json::Value;

use super::answer::{State, json, not_found};
use crate::harness::Harness;

/// How long a wait lasts when the model names no timeout: Codex's.
const WAIT_DEFAULT: Duration = Duration::from_secs(30);
/// The longest wait: Codex's hour. Zero is allowed, to only look.
const WAIT_MAX: Duration = Duration::from_hours(1);

pub(super) async fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    let Wait {
        child_id,
        timeout_ms,
    } = match parsed(arguments) {
        Ok(wait) => wait,
        Err(refused) => return refused,
    };
    // The model's `timeout_ms` is the tool call's edge; it becomes a span here.
    let limit = timeout_ms
        .map_or(WAIT_DEFAULT, Duration::from_millis)
        .min(WAIT_MAX);
    match harness.shared.registry.wait(child_id, Some(limit)).await {
        Some(status) => json(&State::of(&status)),
        None => not_found(child_id),
    }
}
