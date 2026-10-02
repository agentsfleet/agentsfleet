//! A run's tool trace: each call it made and how that call ended, kept with
//! the run's answer so a thread reloaded later still shows what the fleet did.
//!
//! The runner builds the trace and the daemon is the authority on what is
//! stored. Both read the bounds below, so a trace the runner checks with
//! [`ToolTrace::validate`] is one the daemon accepts. The daemon checks again
//! because it cannot trust the runner to have checked.
//!
//! Arguments and output edges are scrubbed runner-side before they reach this
//! type, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use crate::activity::CALL_ID_MAX_BYTES;

/// The largest arguments object one call may carry, encoded.
pub const ARGS_MAX_BYTES: usize = 2048;

/// The longest string value anywhere inside a call's arguments.
pub const ARGS_LEAF_MAX_BYTES: usize = 256;

/// The most lines an output edge may hold.
pub const OUTPUT_EDGE_MAX_LINES: usize = 5;

/// The most bytes an output edge may hold.
pub const OUTPUT_EDGE_MAX_BYTES: usize = 1024;

/// The most calls one trace may list. Calls past it are counted in
/// `omitted_call_count` instead.
pub const TRACE_MAX_CALLS: usize = 200;

/// The largest trace, encoded as the runner sends it.
pub const TRACE_MAX_BYTES: usize = 64 * 1024;

/// How one tool call ended.
///
/// Every call ends exactly once. A call the run never finished — a crash, a
/// kill, a timeout — ends `interrupted`, so no call is left running in a
/// browser or missing from the record.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    /// The tool ran and returned what it was asked for.
    Succeeded,
    /// The tool ran and reported an error, or exited non-zero.
    Failed,
    /// The run ended before the call did.
    Interrupted,
}

/// One call in a run's trace.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolTraceCall<'a> {
    /// Which call this is, 1 to 64 bytes. The runner sends its own counter;
    /// the daemon stores it as `{fence}:{counter}`, the id the live frames of
    /// the same call carry.
    #[serde(borrow)]
    pub call_id: Cow<'a, str>,
    /// Which tool.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// The arguments the call was made with, secret values masked. At most
    /// 2048 bytes encoded, and no string inside longer than 256 bytes.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub arguments: Map<String, Value>,
    /// How the call ended.
    pub status: ToolCallStatus,
    /// The output's first lines, at most 5 lines and 1024 bytes. Absent when
    /// the call returned nothing or the trace ran out of room for it.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub output_head: Option<Cow<'a, str>>,
    /// The output's last lines, at most 5 lines and 1024 bytes. Absent when
    /// the head already holds the whole output.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub output_tail: Option<Cow<'a, str>>,
    /// How many lines the whole output had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_line_count: Option<u64>,
    /// The process's exit code, for a call that ran one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// How long the call took, in milliseconds.
    pub duration_ms: u64,
}

/// Every call one run made, oldest first.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolTrace<'a> {
    /// The calls, at most 200.
    #[serde(borrow)]
    pub calls: Vec<ToolTraceCall<'a>>,
    /// Calls the run made past the 200 listed.
    pub omitted_call_count: u64,
}

/// Which bound a trace broke.
///
/// The spelling is what the daemon logs when it drops a trace, so an operator
/// reading the drop knows which bound to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceRejection {
    /// More than [`TRACE_MAX_CALLS`] calls.
    TooManyCalls,
    /// More than [`TRACE_MAX_BYTES`] encoded.
    TooLarge,
    /// A call id outside 1 to [`CALL_ID_MAX_BYTES`] bytes.
    CallIdUnusable,
    /// An arguments object over [`ARGS_MAX_BYTES`] encoded.
    ArgumentsTooLarge,
    /// A string inside the arguments over [`ARGS_LEAF_MAX_BYTES`].
    ArgumentTooLong,
    /// An output edge over [`OUTPUT_EDGE_MAX_BYTES`] or
    /// [`OUTPUT_EDGE_MAX_LINES`].
    EdgeTooLarge,
    /// Not a trace at all: the JSON is the wrong shape.
    Malformed,
}

impl TraceRejection {
    /// The reason as a drop log spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooManyCalls => "too_many_calls",
            Self::TooLarge => "too_large",
            Self::CallIdUnusable => "call_id_unusable",
            Self::ArgumentsTooLarge => "arguments_too_large",
            Self::ArgumentTooLong => "argument_too_long",
            Self::EdgeTooLarge => "edge_too_large",
            Self::Malformed => "malformed",
        }
    }
}

impl ToolTrace<'_> {
    /// Check every bound, the first broken one answering.
    ///
    /// # Errors
    /// The [`TraceRejection`] naming the bound the trace breaks.
    pub fn validate(&self) -> Result<(), TraceRejection> {
        if self.calls.len() > TRACE_MAX_CALLS {
            return Err(TraceRejection::TooManyCalls);
        }
        self.calls.iter().try_for_each(ToolTraceCall::validate)?;
        if encoded_len(self) > TRACE_MAX_BYTES {
            return Err(TraceRejection::TooLarge);
        }
        Ok(())
    }
}

impl ToolTraceCall<'_> {
    /// Check one call's own bounds.
    fn validate(&self) -> Result<(), TraceRejection> {
        if !(1..=CALL_ID_MAX_BYTES).contains(&self.call_id.len()) {
            return Err(TraceRejection::CallIdUnusable);
        }
        if encoded_len(&self.arguments) > ARGS_MAX_BYTES {
            return Err(TraceRejection::ArgumentsTooLarge);
        }
        if !self.arguments.values().all(leaves_fit) {
            return Err(TraceRejection::ArgumentTooLong);
        }
        let edges = [self.output_head.as_deref(), self.output_tail.as_deref()];
        if !edges.into_iter().flatten().all(edge_fits) {
            return Err(TraceRejection::EdgeTooLarge);
        }
        Ok(())
    }
}

/// A trace as the runner sent it, before anything has read it.
///
/// The report carries its trace this way so that a trace of the wrong shape is
/// refused on its own, after the report's own fields have parsed: the run's
/// answer must never be lost over its tool list. [`RawToolTrace::narrow`] is
/// the one place the bytes become a [`ToolTrace`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawToolTrace<'a>(#[serde(borrow)] &'a RawValue);

impl PartialEq for RawToolTrace<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}

impl Eq for RawToolTrace<'_> {}

impl<'a> RawToolTrace<'a> {
    /// How many bytes the runner sent.
    #[must_use]
    pub fn byte_len(self) -> usize {
        self.0.get().len()
    }

    /// The trace these bytes hold, if it is one and fits every bound.
    ///
    /// The byte bound is checked before parsing, so an oversized trace costs
    /// no parse.
    ///
    /// # Errors
    /// The [`TraceRejection`] naming why the trace cannot be kept.
    pub fn narrow(self) -> Result<ToolTrace<'a>, TraceRejection> {
        let text = self.0.get();
        if text.len() > TRACE_MAX_BYTES {
            return Err(TraceRejection::TooLarge);
        }
        let trace: ToolTrace<'a> =
            serde_json::from_str(text).map_err(|_shape| TraceRejection::Malformed)?;
        trace.validate()?;
        Ok(trace)
    }
}

/// Whether one output edge is within [`OUTPUT_EDGE_MAX_BYTES`] and
/// [`OUTPUT_EDGE_MAX_LINES`].
///
/// Shared with the live `tool_call_completed` frame, whose edges are the same
/// edges the trace keeps.
#[must_use]
pub fn edge_fits(edge: &str) -> bool {
    edge.len() <= OUTPUT_EDGE_MAX_BYTES && edge.lines().count() <= OUTPUT_EDGE_MAX_LINES
}

/// Whether every string inside `value` is within [`ARGS_LEAF_MAX_BYTES`].
///
/// Recursion is bounded by the parser, which refuses documents nested past its
/// own depth limit before a value reaches here.
fn leaves_fit(value: &Value) -> bool {
    match value {
        Value::String(text) => text.len() <= ARGS_LEAF_MAX_BYTES,
        Value::Array(items) => items.iter().all(leaves_fit),
        Value::Object(fields) => fields.values().all(leaves_fit),
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
    }
}

/// How many bytes `value` encodes to as compact JSON.
///
/// Encoding maps with string keys, strings and integers cannot fail; a failure
/// would answer the largest size, which every bound refuses.
fn encoded_len<T: Serialize>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

#[cfg(test)]
#[path = "tool_trace/tests.rs"]
mod tests;
