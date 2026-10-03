//! The live-tail progress frames a run streams while it works.
//!
//! Ephemeral and best-effort: a dropped frame is cosmetic. The durable system of
//! record is the report. Arguments are redacted runner-side before they reach
//! this type, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::tool_trace::{ToolCallStatus, edge_fits};

/// The longest call identity a tool frame may carry.
///
/// A runner's own counter needs a few bytes. The bound leaves room for a
/// provider's call id, should a runner forward one, and keeps one frame from
/// deciding how much of every subscriber's buffer an identity takes. The three
/// `call_id` descriptions spell this number out, because they are published
/// and a constant's name means nothing to a runner's author.
pub const CALL_ID_MAX_BYTES: usize = 64;

/// A tool call began.
///
/// `args_redacted` is opaque, pre-stringified JSON built runner-side AFTER
/// substitution — never the resolved bytes.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallStarted<'a> {
    /// Which tool.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// The redacted arguments.
    #[serde(borrow)]
    pub args_redacted: Cow<'a, str>,
    /// Which call of the run this frame belongs to, 1 to 64 bytes: every frame
    /// of one call carries the same value. Absent from runners that do not name
    /// calls.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<Cow<'a, str>>,
}

/// Which part of the model's output a streamed chunk carries: the answer, or
/// the model's reasoning before it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamTextKind {
    /// User-visible answer text from a provider field.
    Answer,
    /// Reasoning text from a dedicated provider field.
    Reasoning,
}

/// The fleet produced output.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetResponseChunk<'a> {
    /// The text produced.
    #[serde(borrow)]
    pub text: Cow<'a, str>,
    /// Absent on older runners; clients use the saved final reply then.
    #[serde(default)]
    pub text_kind: Option<StreamTextKind>,
    /// Present only on the first safe output chunk; relative to agent runtime invocation.
    #[serde(default)]
    pub first_chunk_after_ms: Option<u64>,
    /// True only for the first delivered chunk of an intact model pass.
    #[serde(default)]
    pub stream_start: bool,
    /// False once a runner lost an earlier chunk in this model pass.
    #[serde(default)]
    pub stream_contiguous: bool,
    /// Zero-based output position for detecting loss after the child pipe.
    #[serde(default)]
    pub stream_seq: u64,
}

/// A tool call finished.
///
/// The outcome fields are absent from runners that do not report one. A
/// reader shows such a call as finished with no outcome, never as a success.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallCompleted<'a> {
    /// Which tool.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// How long it took, in milliseconds.
    pub ms: i64,
    /// Which call of the run this frame belongs to, 1 to 64 bytes: every frame
    /// of one call carries the same value. Absent from runners that do not name
    /// calls.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<Cow<'a, str>>,
    /// How the call ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolCallStatus>,
    /// The output's first lines, at most 5 lines and 1024 bytes.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub output_head: Option<Cow<'a, str>>,
    /// The output's last lines, at most 5 lines and 1024 bytes.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub output_tail: Option<Cow<'a, str>>,
    /// How many lines the whole output had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_line_count: Option<u64>,
    /// The process's exit code, for a call that ran one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// A long-running tool is still working, so a reader's spinner survives it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallProgress<'a> {
    /// Which tool.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// How long it has been running, in milliseconds.
    pub elapsed_ms: i64,
    /// Which call of the run this frame belongs to, 1 to 64 bytes: every frame
    /// of one call carries the same value. Absent from runners that do not name
    /// calls.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<Cow<'a, str>>,
}

/// One progress frame.
//
// The variant name IS the wire discriminator, so the enum is the single source
// for the vocabulary and there are no re-spelled kind strings. Each payload is
// a named struct rather than an inline variant body, matching the Zig union
// field for field — the encoding is identical either way, and the named form
// is what lets each payload carry its own fixture.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityFrame<'a> {
    /// A tool call began.
    #[serde(borrow)]
    ToolCallStarted(ToolCallStarted<'a>),
    /// The fleet produced output.
    #[serde(borrow)]
    FleetResponseChunk(FleetResponseChunk<'a>),
    /// A tool call finished.
    #[serde(borrow)]
    ToolCallCompleted(ToolCallCompleted<'a>),
    /// A long-running tool is still working.
    #[serde(borrow)]
    ToolCallProgress(ToolCallProgress<'a>),
}

impl ActivityFrame<'_> {
    /// The call a tool frame names, if it names one.
    #[must_use]
    pub fn call_id(&self) -> Option<&str> {
        match self {
            Self::ToolCallStarted(body) => body.call_id.as_deref(),
            Self::ToolCallProgress(body) => body.call_id.as_deref(),
            Self::ToolCallCompleted(body) => body.call_id.as_deref(),
            Self::FleetResponseChunk(_) => None,
        }
    }

    /// Whether the call this frame names, if any, is 1 to
    /// [`CALL_ID_MAX_BYTES`] bytes.
    #[must_use]
    pub fn call_id_usable(&self) -> bool {
        self.call_id()
            .is_none_or(|id| (1..=CALL_ID_MAX_BYTES).contains(&id.len()))
    }

    /// Whether a completion's output edges, if any, fit the bounds a stored
    /// trace holds them to. Every other frame carries no edges.
    #[must_use]
    pub fn outcome_usable(&self) -> bool {
        let Self::ToolCallCompleted(body) = self else {
            return true;
        };
        [body.output_head.as_deref(), body.output_tail.as_deref()]
            .into_iter()
            .flatten()
            .all(edge_fits)
    }
}

/// `POST /v1/runners/me/leases/{lease_id}/activity` request, a batch of frames.
///
/// One frame per request today; the array shape lets a later change coalesce
/// without a wire change. The reply is `202` carrying [`ActivityAccepted`].
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityRequest<'a> {
    /// The frames being forwarded.
    #[serde(borrow)]
    pub frames: Vec<ActivityFrame<'a>>,
}

/// The acknowledgement a batch of frames is answered with.
///
/// `202`, because the frames were received and not yet read by anybody. The
/// publish is best-effort and a subscriber seeing a frame is a separate event
/// from this call returning.
// The one field `service_activity.zig` answers, so a runner pointed at either
// daemon reads one shape. The first port of the verb dropped it and answered
// a bare status; the document gate is what noticed.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityAccepted {
    /// Always `true`: a batch this daemon could not accept is refused with a
    /// problem document, never acknowledged with `false`.
    pub ok: bool,
}

#[cfg(test)]
#[path = "activity/tests.rs"]
mod tests;
