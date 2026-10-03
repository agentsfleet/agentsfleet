//! A run's tool trace: each call it made and how that call ended, kept with
//! the run's answer so a thread reloaded later still shows what the fleet did.
//!
//! The runner builds the trace and the daemon is the authority on what is
//! stored. Both read the bounds below, declared on the types with garde, so a
//! trace the runner checks with [`ToolTrace::validate`] is one the daemon
//! accepts. The daemon checks again because it cannot trust the runner to
//! have checked.
//!
//! Arguments and output edges are scrubbed runner-side before they reach this
//! type, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use afd_validate::PathTable;
use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use self::rules::{
    argument_leaves_fit, arguments_fit, call_free_of_nul, call_number, edge, trace_fits,
};

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
/// Every call ends exactly once. A call the run never finished, after a crash,
/// a kill or a timeout, ends `interrupted`. So no call is left running in a
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
// Every string the call carries, so NUL is one rule over the whole call
// rather than a second rule beside each field's own.
#[garde(custom(call_free_of_nul))]
pub struct ToolTraceCall<'a> {
    /// Which call this is: the decimal `call_number` the call's full record is
    /// posted under, from 1. Anything else makes "show all" unable to find the
    /// call, so the trace is dropped. The daemon stores and serves it as
    /// `{fence}:{call_number}`, the id the call's live frames carry.
    #[serde(borrow)]
    #[garde(custom(call_number))]
    pub call_id: Cow<'a, str>,
    /// Which tool.
    #[serde(borrow)]
    #[garde(skip)]
    pub name: Cow<'a, str>,
    /// The arguments the call was made with, secret values masked. At most
    /// 2048 bytes encoded, no string or key inside longer than 256 bytes, and
    /// no NUL character anywhere.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    #[garde(custom(arguments_fit), custom(argument_leaves_fit))]
    pub arguments: Map<String, Value>,
    /// How the call ended.
    #[garde(skip)]
    pub status: ToolCallStatus,
    /// The output's first lines, at most 5 lines and 1024 bytes. Absent when
    /// the call returned nothing or the trace ran out of room for it.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    #[garde(inner(custom(edge)))]
    pub output_head: Option<Cow<'a, str>>,
    /// The output's last lines, at most 5 lines and 1024 bytes. Absent when
    /// the head already holds the whole output.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    #[garde(inner(custom(edge)))]
    pub output_tail: Option<Cow<'a, str>>,
    /// How many lines the whole output had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(skip)]
    pub output_line_count: Option<u64>,
    /// The process's exit code, for a call that ran one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(skip)]
    pub exit_code: Option<i32>,
    /// How long the call took, in milliseconds.
    #[garde(skip)]
    pub duration_ms: u64,
}

/// Every call one run made, oldest first.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(custom(trace_fits))]
pub struct ToolTrace<'a> {
    /// The calls, at most 200.
    #[serde(borrow)]
    #[garde(length(max = TRACE_MAX_CALLS), dive)]
    pub calls: Vec<ToolTraceCall<'a>>,
    /// Calls the run made past the 200 listed.
    #[garde(skip)]
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
    /// A call id that is not a decimal call number from 1 to `i64::MAX`, the
    /// number the call's full record is posted and read under.
    CallIdUnusable,
    /// An arguments object over [`ARGS_MAX_BYTES`] encoded.
    ArgumentsTooLarge,
    /// A string or key inside the arguments over [`ARGS_LEAF_MAX_BYTES`].
    ArgumentTooLong,
    /// An output edge over [`OUTPUT_EDGE_MAX_BYTES`] or
    /// [`OUTPUT_EDGE_MAX_LINES`].
    EdgeTooLarge,
    /// Not a trace at all: the JSON is the wrong shape.
    Malformed,
    /// A string holds a NUL character, which a stored trace cannot: Postgres
    /// `jsonb` refuses `\u0000`, and the trace is written in the statement
    /// that settles the run.
    HoldsNul,
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
            Self::HoldsNul => "holds_nul",
        }
    }

    /// The rejections a rule of this module reports, in the order one wins
    /// when a trace breaks several: the call count is garde's own `length`
    /// and is named by its path in [`BY_PATH`] instead.
    const BY_PRECEDENCE: [Self; 6] = [
        Self::HoldsNul,
        Self::CallIdUnusable,
        Self::ArgumentsTooLarge,
        Self::ArgumentTooLong,
        Self::EdgeTooLarge,
        Self::TooLarge,
    ];

    /// The rejection a garde report names.
    ///
    /// A rule this module wrote reports its own spelling, so it is matched by
    /// message; the call count is matched by the path garde reports it under.
    fn of(report: &garde::Report) -> Self {
        BY_PATH.pick(report).unwrap_or_else(|| {
            Self::BY_PRECEDENCE
                .into_iter()
                .find(|rejection| {
                    report
                        .iter()
                        .any(|(_path, error)| error.message() == rejection.as_str())
                })
                .unwrap_or(Self::Malformed)
        })
    }
}

/// The path garde reports the call count's `length` break under.
const PATH_CALLS: &str = "calls";

/// The one rejection a garde built-in reports: by path, ahead of every other.
const BY_PATH: PathTable<Option<TraceRejection>> =
    PathTable::new(&[(PATH_CALLS, Some(TraceRejection::TooManyCalls))], None);

impl ToolTrace<'_> {
    /// Check every bound the type declares, the highest-precedence break
    /// answering.
    ///
    /// # Errors
    /// The [`TraceRejection`] naming the bound the trace breaks.
    pub fn validate(&self) -> Result<(), TraceRejection> {
        Validate::validate(self).map_err(|report| TraceRejection::of(&report))
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

/// How many bytes `value` encodes to as compact JSON.
///
/// Encoding maps with string keys, strings and integers cannot fail; a failure
/// would answer the largest size, which every bound refuses.
pub(crate) fn encoded_len<T: Serialize>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

#[path = "tool_trace/call_id.rs"]
mod call_id;
#[path = "tool_trace/nul.rs"]
mod nul;
#[path = "tool_trace/rules.rs"]
mod rules;

pub use self::call_id::{CALL_ID_SEPARATOR, fenced_call_id, parse_fenced_call_id};
pub use self::nul::{fields_free_of_nul, free_of_nul};

#[cfg(test)]
#[path = "tool_trace/tests.rs"]
mod tests;
