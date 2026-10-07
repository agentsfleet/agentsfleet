//! `wait_agent`: a spawned child's end, or that it still runs.

use std::time::Duration;

use afr_tools::nested::Wait;
use afr_tools::{ToolOutput, parsed};
use serde_json::Value;

use super::answer::{State, json, not_found};
use crate::harness::Harness;

/// How long a wait lasts when the model names no timeout: Codex's.
const WAIT_MS_DEFAULT: u64 = 30_000;
/// The longest wait: Codex's hour. Zero is allowed, to only look.
const WAIT_MS_MAX: u64 = 3_600_000;

pub(super) async fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    let Wait {
        child_id,
        timeout_ms,
    } = match parsed(arguments) {
        Ok(wait) => wait,
        Err(refused) => return refused,
    };
    let limit = Duration::from_millis(timeout_ms.unwrap_or(WAIT_MS_DEFAULT).min(WAIT_MS_MAX));
    match harness.shared.registry.wait(child_id, Some(limit)).await {
        Some(status) => json(&State::of(&status)),
        None => not_found(child_id),
    }
}
