//! The ledger: what each tool call did, kept in one place.
//!
//! [`Ledger::call`] is the one way a call runs, the shape Exonum's
//! `TopLevelContext::call` gives a transaction: one wrapper guarantees one
//! outcome. A call is numbered from 1 and opens with its `tool_call_started`
//! frame. It ends exactly once: its `tool_call_completed` frame, its trace row
//! and, when its handler returned, its full record. A call whose future is
//! dropped before the handler returned (the lease stopped, the run future was
//! dropped) ends `interrupted`, live and in the trace, so no run ending can
//! leave a call open. Records are held only while they fit what one event may
//! keep, measured as the daemon measures it: one it would refuse is not held
//! to be posted.

use std::borrow::Cow;
use std::time::Instant;

use afd_core::clock::{saturating_millis, saturating_millis_signed};
use afd_wire::activity::{ActivityFrame, ToolCallCompleted, ToolCallStarted};
use afd_wire::tool_detail::{DETAIL_EVENT_MAX_BYTES, ToolCallRecord};
use afd_wire::tool_trace::{ToolCallStatus, ToolTrace};
use afr_providers::Call;
use afr_telemetry::labels::{Tool, ToolOutcome};
use afr_telemetry::record;
use afr_tools::ToolOutput;
use serde_json::{Map, Value};
use tracing::Instrument as _;

use crate::engine::EventSink;
use crate::records::record;
use crate::spans;
use crate::trace::{Outcome, Trace, bounded_arguments};
use afr_secrets::{Clean, Scrub};

pub(crate) const EVENT_CALL_STARTED: &str = "tool_call_started";
pub(crate) const EVENT_CALL_COMPLETED: &str = "tool_call_completed";
/// A full record past the event's budget, kept out of the post. The trace
/// still lists the call, so an operator's "show all" finds no record for it.
const EVENT_RECORD_DROPPED: &str = "tool_record_dropped";

/// Every call one run made.
pub(crate) struct Ledger<'run> {
    lease_id: &'run str,
    sink: &'run dyn EventSink,
    scrub: &'run Scrub,
    trace: Trace,
    records: Vec<ToolCallRecord<'static>>,
    /// What the held records spend of the event's budget.
    spent: usize,
    calls: u64,
}

impl<'run> Ledger<'run> {
    /// Lease `lease_id`'s ledger, sending its frames to `sink` and masking
    /// through `scrub`.
    pub(crate) fn new(lease_id: &'run str, sink: &'run dyn EventSink, scrub: &'run Scrub) -> Self {
        Self {
            lease_id,
            sink,
            scrub,
            trace: Trace::default(),
            records: Vec::new(),
            spent: 0,
            calls: 0,
        }
    }

    /// Runs `call` through `handler` and hands back the scrubbed text the
    /// model reads.
    pub(crate) async fn call(
        &mut self,
        call: &Call,
        handler: impl Future<Output = ToolOutput>,
    ) -> Clean<String> {
        let open = self.open(call);
        let span = spans::execute_tool(&call.name, &open.id);
        open.close(handler.instrument(span).await)
    }

    /// Opens the next call: numbers it, scrubs and bounds its arguments, and
    /// sends its start frame.
    fn open<'a>(&'a mut self, call: &'a Call) -> Opened<'a, 'run> {
        self.calls += 1;
        let number = self.calls;
        let id = number.to_string();
        let shown = self.scrub.clean_json(call.arguments.clone());
        let bounded = bounded_arguments(&shown);
        let args_redacted = serde_json::to_string(&bounded).unwrap_or_default();
        let lease_id = self.lease_id;
        let call_id = id.as_str();
        let tool = call.name.as_str();
        let event = EVENT_CALL_STARTED;
        tracing::debug!(lease_id, call_id, tool, event);
        self.sink
            .emit(ActivityFrame::ToolCallStarted(ToolCallStarted {
                name: Cow::Owned(call.name.clone()),
                args_redacted: Cow::Owned(args_redacted),
                call_id: Some(Cow::Owned(id.clone())),
            }));
        Opened {
            ledger: self,
            number,
            id,
            name: &call.name,
            arguments: Some((shown, bounded)),
            started: Instant::now(),
        }
    }

    /// Holds `record` when it fits what is left of [`DETAIL_EVENT_MAX_BYTES`].
    /// A record past it is dropped and a later, smaller one may still fit, as
    /// the daemon keeps them.
    fn hold(&mut self, record: ToolCallRecord<'static>) {
        let after = self.spent.saturating_add(record.byte_count());
        if after <= DETAIL_EVENT_MAX_BYTES {
            self.spent = after;
            self.records.push(record);
        } else {
            let lease_id = self.lease_id;
            let call_number = record.call_number;
            let bytes = record.byte_count();
            let event = EVENT_RECORD_DROPPED;
            tracing::warn!(lease_id, call_number, bytes, event);
        }
    }

    /// The run's trace, none for a run that called no tool, and every record.
    pub(crate) fn finish(self) -> (Option<ToolTrace<'static>>, Vec<ToolCallRecord<'static>>) {
        (self.trace.finish(), self.records)
    }
}

/// One call between its start frame and its end.
struct Opened<'a, 'run> {
    ledger: &'a mut Ledger<'run>,
    number: u64,
    id: String,
    name: &'a str,
    /// The scrubbed arguments for the record, and their bounded form for the
    /// trace row; taken when the call ends, so it ends once.
    arguments: Option<(Clean<Value>, Map<String, Value>)>,
    started: Instant,
}

impl Opened<'_, '_> {
    /// Ends the call with what its handler returned, and hands back the
    /// scrubbed text the model reads.
    fn close(mut self, output: ToolOutput) -> Clean<String> {
        let text = self.ledger.scrub.clean(output.text);
        let failed = output.error_code.is_some() || output.exit_code.is_some_and(|code| code != 0);
        let status = if failed {
            ToolCallStatus::Failed
        } else {
            ToolCallStatus::Succeeded
        };
        let outcome = Outcome::ended(status, &text, output.exit_code, self.started.elapsed());
        if let Some((shown, bounded)) = self.arguments.take() {
            self.end(bounded, outcome);
            let full = record(self.number, shown, &text);
            self.ledger.hold(full);
        }
        text
    }

    /// Logs the end, sends the end frame and adds the trace row.
    fn end(&mut self, bounded: Map<String, Value>, outcome: Outcome) {
        let lease_id = self.ledger.lease_id;
        let call_id = self.id.as_str();
        let tool = self.name;
        let status = outcome.status;
        let duration_ms = saturating_millis(outcome.elapsed);
        let event = EVENT_CALL_COMPLETED;
        tracing::debug!(lease_id, call_id, tool, ?status, duration_ms, event);
        record::tool_call(Tool::of(tool), tool_outcome(status), outcome.elapsed);
        self.ledger
            .sink
            .emit(ActivityFrame::ToolCallCompleted(ToolCallCompleted {
                name: Cow::Owned(self.name.to_owned()),
                ms: saturating_millis_signed(outcome.elapsed),
                call_id: Some(Cow::Owned(std::mem::take(&mut self.id))),
                status: Some(outcome.status),
                output_head: outcome.head.clone().map(Cow::Owned),
                output_tail: outcome.tail.clone().map(Cow::Owned),
                output_line_count: outcome.line_count,
                exit_code: outcome.exit_code,
            }));
        self.ledger
            .trace
            .push(self.number, self.name, bounded, outcome);
    }
}

/// How a call ended, as the tool-duration family labels it.
const fn tool_outcome(status: ToolCallStatus) -> ToolOutcome {
    match status {
        ToolCallStatus::Succeeded => ToolOutcome::Succeeded,
        ToolCallStatus::Failed => ToolOutcome::Failed,
        ToolCallStatus::Interrupted => ToolOutcome::Interrupted,
    }
}

impl Drop for Opened<'_, '_> {
    fn drop(&mut self) {
        if let Some((_, bounded)) = self.arguments.take() {
            self.end(bounded, Outcome::interrupted(self.started.elapsed()));
        }
    }
}

#[cfg(test)]
#[path = "ledger/tests.rs"]
mod tests;
