//! A call id scoped to its lease, and the report's tool trace on its way to the
//! event row.
//!
//! # A call id is scoped to the lease that sent it
//!
//! The runner numbers a run's tool calls from 1 and repeats the number on every
//! frame of one call. A reclaimed lease re-runs the SAME event
//! (`lease/reclaim.rs`) and the runner's counter starts over, so the dead run's
//! call 1 and the new run's call 1 would carry one id, and a reader pairing
//! calls on that id alone would merge two calls. Prefixing the lease's fence,
//! which is distinct per claim, gives every lease of an event its own ids. The
//! live frames and the stored trace use the same prefix, so a thread reloaded
//! after a run pairs its rows with the frames it watched live.
//!
//! # A trace never costs a run its answer
//!
//! The trace rides the report as raw JSON and is read only here, after the
//! report has parsed. One that is not a trace, or that breaks a bound, is
//! dropped and logged, and the report settles without it.

use afd_wire::tool_trace::{RawToolTrace, fenced_call_id};

/// A report's trace was not stored; the run settles without it.
const EVENT_TRACE_DROPPED: &str = "report_tool_trace_dropped";

/// Who a trace belongs to, for the line a dropped one is logged under.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TraceOwner<'a> {
    /// The fleet that ran.
    pub fleet_id: &'a str,
    /// The event the run answered.
    pub event_id: &'a str,
    /// The fence the lease holds, which scopes every call id.
    pub fence: i64,
}

/// The trace as the event row stores it, or nothing.
///
/// Nothing when the runner sent none, and nothing — logged — when what it sent
/// is not a trace or breaks a bound. Each call id comes back fenced.
#[must_use]
pub(crate) fn stored(raw: Option<RawToolTrace<'_>>, owner: TraceOwner<'_>) -> Option<String> {
    let raw = raw?;
    let mut trace = match raw.narrow() {
        Ok(trace) => trace,
        Err(rejection) => {
            let fleet_id = owner.fleet_id;
            let event_id = owner.event_id;
            let reason = rejection.as_str();
            let bytes = raw.byte_len();
            tracing::warn!(
                fleet_id,
                agentsfleet_event_id = event_id,
                reason,
                bytes,
                event = EVENT_TRACE_DROPPED,
                "the run's tool trace was not stored; the report settles without it"
            );
            return None;
        }
    };
    for call in &mut trace.calls {
        call.call_id = fenced_call_id(owner.fence, &call.call_id).into();
    }
    // A narrowed trace is strings, integers and a JSON object, which always
    // encode; the `ok()` is the signature's, not a reachable drop.
    serde_json::to_string(&trace).ok()
}

#[cfg(test)]
#[path = "tool_trace/tests.rs"]
mod tests;
