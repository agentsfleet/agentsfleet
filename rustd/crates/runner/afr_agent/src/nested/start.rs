//! Starting a child from a loop: the run's registry narrows the loop's
//! selection to the names asked for, applies the depth cap and the run's
//! caps, and a refusal is logged by its code.

use afr_tools::nested::Task;
use guffbolt::ToolOutput;
use guffbolt::agents::{Parent, Refused};

use crate::harness::Harness;

pub(super) const EVENT_CHILD_REFUSED: &str = "child_refused";

/// Reserves a child of `harness` over `task`, and answers its id.
///
/// # Errors
/// A tool asked for that this loop does not hold, or the run's caps.
pub(super) fn start(harness: &Harness<'_, '_>, task: Task) -> Result<u64, ToolOutput> {
    let parent = Parent {
        depth: harness.depth,
        stop: &harness.stop,
        tools: harness.selection,
    };
    harness
        .shared
        .registry
        .start(parent, task.task, task.tools.as_deref())
        .map_err(|refused| logged(harness, refused))
}

/// A refusal, logged by its code and never by the task.
fn logged(harness: &Harness<'_, '_>, refused: Refused) -> ToolOutput {
    let run_id = harness.shared.run_id;
    let error_code = refused.code().as_str();
    let event = EVENT_CHILD_REFUSED;
    tracing::info!(run_id, error_code, event);
    refused.into()
}
