//! The vocabulary bridge: one runner frame as the dashboard's channel carries it.
//!
//! Split from [`super`], which authorizes and publishes; this decides only the
//! shape a frame takes on `fleet:{id}:activity`.
//!
//! Call ids are published fenced, `{fence}:{call_id}`; why is
//! `lease/tool_trace/mod.rs`'s to say, because the stored trace uses the same ids.

use af_core::id::Uuid7;
use af_wire::activity::ActivityFrame;
use af_wire::tool_trace::{ToolCallStatus, fenced_call_id};
use serde::Serialize;
use serde_json::value::RawValue;

use super::Target;

/// One frame as the dashboard reads it.
///
/// Tagged by `kind`, which is the discriminator `events.ts` switches on. The
/// payload field names are the ones it reads, so renaming one is a dashboard
/// change.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Published<'a> {
    TaskDecisionStarted {
        event_id: &'a Uuid7,
        progress: &'a af_wire::activity::TaskActivity,
    },
    TaskDecisionCompleted {
        event_id: &'a Uuid7,
        progress: &'a af_wire::activity::TaskActivity,
    },
    TaskTransferProgress {
        event_id: &'a Uuid7,
        progress: &'a af_wire::activity::TaskActivity,
    },
    ContextProgress {
        event_id: &'a Uuid7,
        #[serde(flatten)]
        progress: af_wire::context::ContextProgress,
    },
    ToolCallStarted {
        event_id: &'a Uuid7,
        name: &'a str,
        /// Spliced in verbatim, NOT re-encoded — see [`Published::of`].
        args_redacted: &'a RawValue,
        call_id: String,
    },
    ToolCallProgress {
        event_id: &'a Uuid7,
        name: &'a str,
        elapsed_ms: i64,
        call_id: String,
    },
    /// The one rename in the bridge: `fleet_response_chunk` on the wire.
    #[serde(rename = "chunk")]
    Chunk {
        event_id: &'a Uuid7,
        text: &'a str,
        text_kind: af_wire::activity::StreamTextKind,
        /// A subscriber trusts raw model text only from this first frame.
        stream_start: bool,
        /// A dropped runner chunk makes later bytes unsafe to classify.
        stream_contiguous: bool,
        /// Zero-based output position, retained across lossy forwarding.
        stream_seq: u64,
    },
    ToolCallCompleted {
        event_id: &'a Uuid7,
        name: &'a str,
        ms: i64,
        call_id: String,
        status: ToolCallStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_head: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_tail: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_line_count: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
}

impl<'a> Published<'a> {
    /// The dashboard's shape for one wire frame.
    ///
    /// Total over [`ActivityFrame`], so a frame variant added upstream fails to
    /// compile here until its channel name is decided — which is the reason
    /// this is a match rather than a serde re-tag.
    ///
    /// # Errors
    /// Reports arguments that are not well-formed JSON. Only the started frame
    /// can fail, because it is the only one carrying a nested document.
    pub(super) fn of(
        target: &'a Target,
        frame: &'a ActivityFrame<'a>,
    ) -> Result<Self, serde_json::Error> {
        let event_id = &target.event_id;
        let call_id = |call: &str| fenced_call_id(target.fence, call);
        Ok(match frame {
            ActivityFrame::TaskDecisionStarted(progress) => {
                Self::TaskDecisionStarted { event_id, progress }
            }
            ActivityFrame::TaskDecisionCompleted(progress) => {
                Self::TaskDecisionCompleted { event_id, progress }
            }
            ActivityFrame::TaskTransferProgress(progress) => {
                Self::TaskTransferProgress { event_id, progress }
            }
            ActivityFrame::ContextProgress(progress) => Self::ContextProgress {
                event_id,
                progress: *progress,
            },
            ActivityFrame::ToolCallStarted(body) => Self::ToolCallStarted {
                event_id,
                name: &body.name,
                args_redacted: serde_json::from_str(&body.args_redacted)?,
                call_id: call_id(&body.call_id),
            },
            ActivityFrame::ToolCallProgress(body) => Self::ToolCallProgress {
                event_id,
                name: &body.name,
                elapsed_ms: body.elapsed_ms,
                call_id: call_id(&body.call_id),
            },
            ActivityFrame::FleetResponseChunk(body) => Self::Chunk {
                event_id,
                text: &body.text,
                text_kind: body.text_kind,
                stream_start: body.stream_start && body.stream_seq == 0 && target.timing_eligible,
                stream_contiguous: body.stream_contiguous && target.timing_eligible,
                stream_seq: body.stream_seq,
            },
            ActivityFrame::ToolCallCompleted(body) => Self::ToolCallCompleted {
                event_id,
                name: &body.name,
                ms: body.ms,
                call_id: call_id(&body.call_id),
                status: body.status,
                output_head: body.output_head.as_deref(),
                output_tail: body.output_tail.as_deref(),
                output_line_count: body.output_line_count,
                exit_code: body.exit_code,
            },
        })
    }
}
