//! How a call ended, the output edges a frame and the trace show, and the run's
//! trace under the bounds `afd_wire::tool_trace` declares.
//!
//! The daemon re-checks every bound and drops a trace that breaks one, so the
//! runner builds one that cannot: past `TRACE_MAX_CALLS` a call is counted as
//! omitted, and past `TRACE_MAX_BYTES` a call keeps its row without edges, or
//! is counted as omitted when even that row does not fit.

use std::borrow::Cow;
use std::time::Duration;

use afd_wire::tool_trace::{
    ARGS_LEAF_MAX_BYTES, ARGS_MAX_BYTES, OUTPUT_EDGE_MAX_BYTES, OUTPUT_EDGE_MAX_LINES,
    TRACE_MAX_BYTES, TRACE_MAX_CALLS, ToolCallStatus, ToolTrace, ToolTraceCall,
};
use serde_json::{Map, Value};

/// What a NUL character becomes: no stored trace or record may hold one.
const NUL_STAND_IN: &str = "\u{fffd}";

/// How one call ended, as its completion frame and its trace row carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub(crate) status: ToolCallStatus,
    pub(crate) head: Option<String>,
    pub(crate) tail: Option<String>,
    pub(crate) line_count: Option<u64>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) elapsed: Duration,
}

impl Outcome {
    /// A call that returned `output`.
    pub(crate) fn ended(
        status: ToolCallStatus,
        output: &str,
        exit_code: Option<i32>,
        elapsed: Duration,
    ) -> Self {
        let output = without_nul(output);
        let head = edge_head(&output);
        let tail = (head.len() < output.len()).then(|| edge_tail(&output));
        Self {
            status,
            head: (!head.is_empty()).then(|| head.to_owned()),
            tail: tail.filter(|tail| !tail.is_empty()).map(str::to_owned),
            line_count: Some(output.lines().count() as u64),
            exit_code,
            elapsed,
        }
    }

    /// A call the run ended before it did.
    pub(crate) fn interrupted(elapsed: Duration) -> Self {
        Self {
            status: ToolCallStatus::Interrupted,
            head: None,
            tail: None,
            line_count: None,
            exit_code: None,
            elapsed,
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
    let head = &output[..lines_end];
    &head[..head.floor_char_boundary(OUTPUT_EDGE_MAX_BYTES)]
}

/// The output's last lines, within both edge bounds.
fn edge_tail(output: &str) -> &str {
    let body = output.strip_suffix('\n').unwrap_or(output);
    let start = body
        .rmatch_indices('\n')
        .nth(OUTPUT_EDGE_MAX_LINES - 1)
        .map_or(0, |(newline, _)| newline + 1);
    let tail = &output[start..];
    let cut = tail.len().saturating_sub(OUTPUT_EDGE_MAX_BYTES);
    &tail[tail.ceil_char_boundary(cut)..]
}

/// `text` with every NUL replaced.
pub(crate) fn without_nul(text: &str) -> Cow<'_, str> {
    if text.contains('\0') {
        Cow::Owned(text.replace('\0', NUL_STAND_IN))
    } else {
        Cow::Borrowed(text)
    }
}

/// `arguments` as a trace row and a start frame may carry them: every key and
/// string cut to its leaf bound, and the whole emptied when it still encodes
/// past `ARGS_MAX_BYTES`. Arguments that are not an object carry nothing.
pub(crate) fn bounded_arguments(arguments: &Value) -> Map<String, Value> {
    let Value::Object(fields) = arguments else {
        return Map::new();
    };
    let bounded: Map<String, Value> = fields
        .iter()
        .map(|(key, value)| (leaf(key), bounded_value(value)))
        .collect();
    if encoded_len(&bounded) > ARGS_MAX_BYTES {
        Map::new()
    } else {
        bounded
    }
}

fn bounded_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(leaf(text)),
        Value::Array(items) => Value::Array(items.iter().map(bounded_value).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, field)| (leaf(key), bounded_value(field)))
                .collect(),
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
    }
}

/// `text` cut to `ARGS_LEAF_MAX_BYTES` on a character boundary, NUL replaced.
fn leaf(text: &str) -> String {
    let text = without_nul(text);
    text[..text.floor_char_boundary(ARGS_LEAF_MAX_BYTES)].to_owned()
}

/// How many bytes `value` encodes to; a failure answers the largest size,
/// which every bound refuses.
pub(crate) fn encoded_len<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

/// The run's trace as it grows.
#[derive(Debug)]
pub(crate) struct Trace {
    calls: Vec<ToolTraceCall<'static>>,
    omitted: u64,
    room: usize,
}

impl Default for Trace {
    fn default() -> Self {
        let frame = ToolTrace {
            calls: Vec::new(),
            omitted_call_count: u64::MAX,
        };
        Self {
            calls: Vec::new(),
            omitted: 0,
            room: TRACE_MAX_BYTES.saturating_sub(encoded_len(&frame)),
        }
    }
}

impl Trace {
    /// Adds call `number`'s row.
    pub(crate) fn push(
        &mut self,
        number: u64,
        name: &str,
        arguments: Map<String, Value>,
        outcome: &Outcome,
    ) {
        if self.calls.len() >= TRACE_MAX_CALLS {
            self.omitted += 1;
            return;
        }
        let mut row = ToolTraceCall {
            call_id: Cow::Owned(number.to_string()),
            name: Cow::Owned(name.to_owned()),
            arguments,
            status: outcome.status,
            output_head: outcome.head.clone().map(Cow::Owned),
            output_tail: outcome.tail.clone().map(Cow::Owned),
            output_line_count: outcome.line_count,
            exit_code: outcome.exit_code,
            duration_ms: afd_core::clock::saturating_millis(outcome.elapsed),
        };
        // A comma joins every row after the first.
        let joint = usize::from(!self.calls.is_empty());
        let mut cost = encoded_len(&row) + joint;
        if cost > self.room {
            row.output_head = None;
            row.output_tail = None;
            cost = encoded_len(&row) + joint;
        }
        if cost > self.room {
            self.omitted += 1;
            return;
        }
        self.room -= cost;
        self.calls.push(row);
    }

    /// The finished trace, or none for a run that called no tool.
    pub(crate) fn finish(self) -> Option<ToolTrace<'static>> {
        (!self.calls.is_empty() || self.omitted > 0).then_some(ToolTrace {
            calls: self.calls,
            omitted_call_count: self.omitted,
        })
    }
}

#[cfg(test)]
#[path = "trace/tests.rs"]
mod tests;
