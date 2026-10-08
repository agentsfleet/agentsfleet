//! Starting a child from a loop: its tools are the loop's, narrowed to the
//! names asked for and to none of the six at the depth cap, and the run's
//! caps decide whether it may start.

use afr_tools::nested::{NESTED, Task};
use afr_tools::{ToolErrorCode, ToolOutput};

use super::registry::NESTED_DEPTH_MAX;
use crate::harness::Harness;

pub(super) const EVENT_CHILD_REFUSED: &str = "child_refused";
/// What a child asking for a tool its parent lacks reads.
const NOT_HELD: &str = "is not one of this run's tools";
/// What a child past the run's caps reads.
const CAP_REACHED: &str =
    "as many children run, or were started, as one run may; wait for one to end";

/// Reserves a child of `harness` over `task`, and answers its id.
///
/// # Errors
/// A tool asked for that this loop does not hold, or the run's caps.
pub(super) fn start(harness: &Harness<'_, '_>, task: Task) -> Result<u64, ToolOutput> {
    let depth = harness.depth + 1;
    let selection = match task.tools {
        Some(names) => match harness.selection.narrowed(&names) {
            Ok(selection) => selection,
            // The name the parent lacks is the refusal's whole content; no
            // error chain leaves here.
            Err(missing) => {
                let detail = format!("{missing} {NOT_HELD}");
                return Err(refused(harness, ToolErrorCode::ChildToolNotHeld, &detail));
            }
        },
        None => harness.selection.clone(),
    };
    let selection = if depth >= NESTED_DEPTH_MAX {
        selection.without(&NESTED)
    } else {
        selection
    };
    harness
        .shared
        .registry
        .start(depth, task.task, selection, &harness.stop)
        .map_err(|code| refused(harness, code, CAP_REACHED))
}

/// A refusal, logged by its code and never by the task.
fn refused(harness: &Harness<'_, '_>, code: ToolErrorCode, detail: &str) -> ToolOutput {
    let lease_id = harness.shared.lease_id;
    let error_code = code.as_str();
    let event = EVENT_CHILD_REFUSED;
    tracing::info!(lease_id, error_code, event);
    ToolOutput::failed(code, detail)
}
