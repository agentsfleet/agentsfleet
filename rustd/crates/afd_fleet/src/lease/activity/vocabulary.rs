//! Every frame the activity channel publishes, rendered, for the suite that
//! grades the published stream description against them.
//!
//! The description a client reads names each frame kind; the generated
//! document lives in `afd_api`, which this crate cannot depend on, so the
//! rendered frames are handed across instead. [`slot`] is an exhaustive match
//! over [`ActivityFrame`]: a new variant does not compile here until it has a
//! slot, and the grading suite fails until [`one_of_each`] builds it.

use std::borrow::Cow;

use afd_wire::activity::{
    ActivityFrame, FleetResponseChunk, ToolCallCompleted, ToolCallProgress, ToolCallStarted,
};
use serde_json::Value;

use super::Target;
use super::published::Published;

/// How many variants [`ActivityFrame`] has: one per slot `slot` hands out.
pub const ACTIVITY_VARIANTS: usize = 5;

/// The tool every sample frame names.
const TOOL: Cow<'static, str> = Cow::Borrowed("shell");

/// The call id every sample tool frame carries, so the published id shows.
const CALL: Cow<'static, str> = Cow::Borrowed("1");

/// One frame as the runner sends it and as the channel publishes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFrame {
    /// Which [`ActivityFrame`] variant it is, below [`ACTIVITY_VARIANTS`].
    pub slot: usize,
    /// The frame as the runner sends it on the wire.
    pub sent: Value,
    /// The frame as the dashboard's channel carries it.
    pub published: Value,
}

/// One frame of every kind, rendered under `target`.
///
/// # Errors
/// Reports a sample that does not serialize, which no sample here does.
pub fn rendered_frames(target: &Target) -> serde_json::Result<Vec<RenderedFrame>> {
    one_of_each()
        .iter()
        .map(|frame| {
            Ok(RenderedFrame {
                slot: slot(frame),
                sent: serde_json::to_value(frame)?,
                published: serde_json::to_value(Published::of(target, frame)?)?,
            })
        })
        .collect()
}

/// Which variant `frame` is. The guard, not the source of the kinds.
const fn slot(frame: &ActivityFrame<'_>) -> usize {
    match frame {
        ActivityFrame::ToolCallStarted(_) => 0,
        ActivityFrame::FleetResponseChunk(_) => 1,
        ActivityFrame::ToolCallCompleted(_) => 2,
        ActivityFrame::ToolCallProgress(_) => 3,
        ActivityFrame::ContextProgress(_) => 4,
    }
}

/// One frame of each kind, the smallest each can be.
fn one_of_each() -> [ActivityFrame<'static>; ACTIVITY_VARIANTS] {
    [
        ActivityFrame::ToolCallStarted(ToolCallStarted {
            name: TOOL,
            args_redacted: Cow::Borrowed("{}"),
            call_id: CALL,
        }),
        ActivityFrame::FleetResponseChunk(FleetResponseChunk {
            text: TOOL,
            text_kind: afd_wire::activity::StreamTextKind::Answer,
            first_chunk_after_ms: None,
            stream_start: false,
            stream_contiguous: false,
            stream_seq: 0,
        }),
        ActivityFrame::ToolCallCompleted(ToolCallCompleted {
            name: TOOL,
            ms: 0,
            call_id: CALL,
            status: afd_wire::tool_trace::ToolCallStatus::Succeeded,
            output_head: None,
            output_tail: None,
            output_line_count: None,
            exit_code: None,
        }),
        ActivityFrame::ToolCallProgress(ToolCallProgress {
            name: TOOL,
            elapsed_ms: 0,
            call_id: CALL,
        }),
        ActivityFrame::ContextProgress(afd_wire::context::ContextProgress {
            generation: 1,
            state: afd_wire::context::ContextState::Summarizing,
        }),
    ]
}
