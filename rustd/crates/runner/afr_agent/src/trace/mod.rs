//! How a call ended, the output edges a frame and the trace show, and the run's
//! trace under the bounds `af_wire::tool_trace` declares.
//!
//! The daemon re-checks every bound and drops a trace that breaks one, so the
//! runner builds rows that cannot: each row's arguments and edges are cut to
//! their own bounds. No count or size caps the trace itself; every call is
//! listed, and the report's frame bounds the whole.
//!
//! Everything here takes [`Clean`] text, so the scrub has already masked every
//! secret and NUL before a bound is applied.

use std::borrow::Cow;
use std::time::Duration;

use af_wire::tool_trace::{
    ARGS_LEAF_MAX_BYTES, ARGS_MAX_BYTES, FileChange, OUTPUT_EDGE_MAX_BYTES, OUTPUT_EDGE_MAX_LINES,
    ToolCallStatus, ToolTrace, ToolTraceCall, encoded_len,
};
use serde_json::{Map, Value};

use af_secrets::{Clean, json};

/// How one call ended, as its completion frame and its trace row carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub(crate) status: ToolCallStatus,
    pub(crate) head: Option<String>,
    pub(crate) tail: Option<String>,
    pub(crate) line_count: Option<u64>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) elapsed: Duration,
    /// For an edit that succeeded, each file it changed; empty otherwise.
    pub(crate) changes: Vec<FileChange<'static>>,
}

impl Outcome {
    /// A call that returned `output`.
    pub(crate) fn ended(
        status: ToolCallStatus,
        output: &Clean<String>,
        exit_code: Option<i32>,
        elapsed: Duration,
    ) -> Self {
        let head = edge_head(output);
        let tail = (head.len() < output.len()).then(|| edge_tail(output));
        Self {
            status,
            head: (!head.is_empty()).then(|| head.to_owned()),
            tail: tail.filter(|tail| !tail.is_empty()).map(str::to_owned),
            line_count: Some(u64::try_from(output.lines().count()).unwrap_or(u64::MAX)),
            exit_code,
            elapsed,
            changes: Vec::new(),
        }
    }

    /// A call the run ended before it did.
    pub(crate) const fn interrupted(elapsed: Duration) -> Self {
        Self {
            status: ToolCallStatus::Interrupted,
            head: None,
            tail: None,
            line_count: None,
            exit_code: None,
            elapsed,
            changes: Vec::new(),
        }
    }
}

/// The output's first lines, within both edge bounds.
fn edge_head(output: &str) -> &str {
    let lines_end = output
        .split_inclusive('\n')
        .take(OUTPUT_EDGE_MAX_LINES)
        .map(str::len)
        .sum::<usize>();
    let head = output.get(..lines_end).unwrap_or_default();
    head.get(..head.floor_char_boundary(OUTPUT_EDGE_MAX_BYTES))
        .unwrap_or_default()
}

/// The output's last lines, within both edge bounds.
fn edge_tail(output: &str) -> &str {
    let body = output.strip_suffix('\n').unwrap_or(output);
    let start = body
        .rmatch_indices('\n')
        .nth(OUTPUT_EDGE_MAX_LINES - 1)
        .map_or(0, |(newline, _)| newline + 1);
    let tail = output.get(start..).unwrap_or_default();
    let cut = tail.len().saturating_sub(OUTPUT_EDGE_MAX_BYTES);
    tail.get(tail.ceil_char_boundary(cut)..).unwrap_or_default()
}

/// `arguments` as a trace row and a start frame may carry them: every key and
/// string cut to its leaf bound, and the whole emptied when it still encodes
/// past `ARGS_MAX_BYTES`. Arguments that are not an object carry nothing.
pub(crate) fn bounded_arguments(arguments: &Clean<Value>) -> Map<String, Value> {
    let Value::Object(fields) = &**arguments else {
        return Map::new();
    };
    let mut bounded = Value::Object(fields.clone());
    json::rewrite(&mut bounded, &leaf);
    match bounded {
        Value::Object(bounded) if encoded_len(&bounded) <= ARGS_MAX_BYTES => bounded,
        Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Array(_)
        | Value::Object(_) => Map::new(),
    }
}

/// `text` cut to `ARGS_LEAF_MAX_BYTES` on a character boundary, when it is
/// longer.
fn leaf(text: &str) -> Option<String> {
    let cut = text.floor_char_boundary(ARGS_LEAF_MAX_BYTES);
    (cut < text.len()).then(|| text.get(..cut).unwrap_or_default().to_owned())
}

/// The run's trace as it grows.
#[derive(Debug, Default)]
pub(crate) struct Trace {
    calls: Vec<ToolTraceCall<'static>>,
}

impl Trace {
    /// Adds call `number`'s row.
    pub(crate) fn push(
        &mut self,
        number: u64,
        name: &str,
        arguments: Map<String, Value>,
        outcome: Outcome,
    ) {
        self.calls.push(ToolTraceCall {
            call_id: Cow::Owned(number.to_string()),
            name: Cow::Owned(name.to_owned()),
            arguments,
            status: outcome.status,
            output_head: outcome.head.map(Cow::Owned),
            output_tail: outcome.tail.map(Cow::Owned),
            output_line_count: outcome.line_count,
            exit_code: outcome.exit_code,
            duration_ms: af_core::clock::saturating_millis(outcome.elapsed),
            changes: outcome.changes,
        });
    }

    /// The finished trace, including a known empty set when no tool ran.
    pub(crate) fn finish(self) -> ToolTrace<'static> {
        ToolTrace { calls: self.calls }
    }
}

#[cfg(test)]
mod tests;
