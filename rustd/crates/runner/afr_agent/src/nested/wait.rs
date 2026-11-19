//! `wait_agent`: a spawned child's end, or that it still runs.

use std::time::Duration;

use afr_tools::nested::Wait;
use afr_tools::parsed;
use guffbolt::ToolOutput;
use guffbolt::agents::{State, not_found};
use serde_json::Value;

use super::answer::json;
use crate::harness::Harness;

/// What `wait_agent` does, as the model reads it.
pub(super) const DESCRIPTION: &str = "Wait for a spawned child to end and read its answer. \
        Answers its status, with the answer once it is done, and running when the timeout \
        passed first.";
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
    harness
        .shared
        .registry
        .wait(child_id, Some(limit))
        .await
        .map_or_else(|| not_found(child_id), |status| json(&State::of(&status)))
}
