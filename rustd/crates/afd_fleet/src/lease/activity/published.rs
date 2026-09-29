//! The vocabulary bridge: one runner frame as the dashboard's channel carries it.
//!
//! Split from [`super`], which authorizes and publishes; this decides only the
//! shape a frame takes on `fleet:{id}:activity`.
//!
//! # A call id is scoped to the lease that sent it
//!
//! The runner numbers a run's tool calls from 1 and repeats the number on every
//! frame of one call. A reclaimed lease re-runs the SAME event
//! (`lease/reclaim.rs`) and the runner's counter starts over, so the dead run's
//! call 1 and the new run's call 1 would publish one id, and a reader pairing
//! frames on that id alone would merge two calls. Prefixing the lease's fence,
//! which is distinct per claim, gives every lease of an event its own ids.

use afd_wire::activity::ActivityFrame;
use serde::Serialize;
use serde_json::value::RawValue;

use super::Target;

/// Ends the fence in a published `call_id`.
///
/// A fence renders as a decimal integer, which never holds `:`, so the first
/// `:` always ends it whatever the runner's own id carries.
const CALL_ID_SEPARATOR: char = ':';

/// One frame as the dashboard reads it.
///
/// Tagged by `kind`, which is the discriminator `events.ts` switches on. The
/// payload field names are the Zig's, because the consumer is unchanged.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Published<'a> {
    ToolCallStarted {
        event_id: &'a str,
        name: &'a str,
        /// Spliced in verbatim, NOT re-encoded — see [`Published::of`].
        args_redacted: &'a RawValue,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
    },
    ToolCallProgress {
        event_id: &'a str,
        name: &'a str,
        elapsed_ms: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
    },
    /// The one rename in the bridge: `fleet_response_chunk` on the wire.
    #[serde(rename = "chunk")]
    Chunk {
        event_id: &'a str,
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        text_kind: Option<afd_wire::activity::StreamTextKind>,
        /// A subscriber trusts raw model text only from this first frame.
        stream_start: bool,
        /// A dropped runner chunk makes later bytes unsafe to classify.
        stream_contiguous: bool,
        /// Zero-based output position, retained across lossy forwarding.
        stream_seq: u64,
    },
    ToolCallCompleted {
        event_id: &'a str,
        name: &'a str,
        ms: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
    },
}

impl<'a> Published<'a> {
    /// The dashboard's shape for one wire frame.
    ///
    /// Total over [`ActivityFrame`], so a frame variant added upstream fails to
    /// compile here until its channel name is decided — which is the property
    /// the Zig gets from its exhaustive `switch` and the reason this is a match
    /// rather than a serde re-tag.
    ///
    /// # Errors
    /// Reports arguments that are not well-formed JSON. Only the started frame
    /// can fail, because it is the only one carrying a nested document.
    pub(super) fn of(
        target: &'a Target,
        frame: &'a ActivityFrame<'a>,
    ) -> core::result::Result<Self, serde_json::Error> {
        let event_id = target.event_id.as_str();
        let call_id = fenced_call_id(target.fence, frame.call_id());
        Ok(match frame {
            ActivityFrame::ToolCallStarted(body) => Self::ToolCallStarted {
                event_id,
                name: &body.name,
                args_redacted: serde_json::from_str(&body.args_redacted)?,
                call_id,
            },
            ActivityFrame::ToolCallProgress(body) => Self::ToolCallProgress {
                event_id,
                name: &body.name,
                elapsed_ms: body.elapsed_ms,
                call_id,
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
                call_id,
            },
        })
    }
}

/// The runner's call id as the channel publishes it: `{fence}:{call_id}`.
///
/// A frame that names no call publishes none; the id is never invented.
pub(super) fn fenced_call_id(fence: i64, call_id: Option<&str>) -> Option<String> {
    call_id.map(|call| format!("{fence}{CALL_ID_SEPARATOR}{call}"))
}
