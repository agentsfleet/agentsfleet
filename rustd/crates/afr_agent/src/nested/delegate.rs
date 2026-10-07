//! `delegate`: a child run to its answer, which is the call's output.

use afr_tools::nested::Task;
use afr_tools::{ToolErrorCode, ToolOutput, parsed};
use serde_json::Value;

use super::registry::Status;
use super::start::start;
use crate::harness::Harness;

/// What a delegated child that was interrupted reads back as.
const INTERRUPTED: &str = "was interrupted before it answered";
/// What a delegated child whose end never arrived reads back as; the
/// registry keeps every child, so none is.
const LOST: &str = "ended without a status";

pub(super) async fn run(harness: &Harness<'_, '_>, arguments: &Value) -> ToolOutput {
    let task: Task = match parsed(arguments) {
        Ok(task) => task,
        Err(refused) => return refused,
    };
    let id = match start(harness, task) {
        Ok(id) => id,
        Err(refused) => return refused,
    };
    match harness.shared.registry.wait(id, None).await {
        Some(Status::Done(answer)) => ToolOutput::succeeded(answer),
        Some(Status::Failed(detail)) => ToolOutput::failed(ToolErrorCode::ChildFailed, &detail),
        Some(Status::Interrupted) => ToolOutput::failed(
            ToolErrorCode::Interrupted,
            &format!("child {id} {INTERRUPTED}"),
        ),
        Some(Status::Running) | None => {
            ToolOutput::failed(ToolErrorCode::ChildFailed, &format!("child {id} {LOST}"))
        }
    }
}
