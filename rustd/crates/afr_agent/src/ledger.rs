//! The ledger: what each tool call did, kept in one place.
//!
//! A call is numbered from 1 and opens with its `tool_call_started` frame. It
//! ends exactly once: its `tool_call_completed` frame, its trace row and, when
//! its handler returned, its full record. An [`Opened`] dropped before it
//! closed — the lease stopped, the run future was dropped — ends its call
//! `interrupted`, live and in the trace, so no run ending can leave a call open.

use std::borrow::Cow;
use std::time::Instant;

use afd_core::clock::saturating_millis_signed;
use afd_wire::activity::{ActivityFrame, ToolCallCompleted, ToolCallStarted};
use afd_wire::tool_detail::ToolCallRecord;
use afd_wire::tool_trace::{ToolCallStatus, ToolTrace};
use afr_providers::Call;
use afr_tools::ToolOutput;
use serde_json::{Map, Value};

use crate::engine::EventSink;
use crate::records::record;
use crate::scrub::{Clean, Scrub};
use crate::trace::{Outcome, Trace, bounded_arguments};

/// Every call one run made.
pub(crate) struct Ledger<'run> {
    sink: &'run dyn EventSink,
    scrub: &'run Scrub,
    trace: Trace,
    records: Vec<ToolCallRecord<'static>>,
    calls: u64,
}

impl<'run> Ledger<'run> {
    /// A ledger sending its frames to `sink`, masking through `scrub`.
    pub(crate) fn new(sink: &'run dyn EventSink, scrub: &'run Scrub) -> Self {
        Self {
            sink,
            scrub,
            trace: Trace::default(),
            records: Vec::new(),
            calls: 0,
        }
    }

    /// Opens the next call: numbers it, scrubs and bounds its arguments, and
    /// sends its start frame.
    pub(crate) fn open<'a>(&'a mut self, call: &'a Call) -> Opened<'a, 'run> {
        self.calls += 1;
        let number = self.calls;
        let id = number.to_string();
        let shown = self.scrub.clean_json(call.arguments.clone());
        let bounded = bounded_arguments(&shown);
        let args_redacted = serde_json::to_string(&bounded).unwrap_or_default();
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

    /// The run's trace, none for a run that called no tool, and every record.
    pub(crate) fn finish(self) -> (Option<ToolTrace<'static>>, Vec<ToolCallRecord<'static>>) {
        (self.trace.finish(), self.records)
    }
}

/// One call between its start frame and its end.
pub(crate) struct Opened<'a, 'run> {
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
    pub(crate) fn close(mut self, output: ToolOutput) -> Clean<String> {
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
            self.ledger.records.push(full);
        }
        text
    }

    /// Sends the end frame and adds the trace row.
    fn end(&mut self, bounded: Map<String, Value>, outcome: Outcome) {
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

impl Drop for Opened<'_, '_> {
    fn drop(&mut self) {
        if let Some((_, bounded)) = self.arguments.take() {
            self.end(bounded, Outcome::interrupted(self.started.elapsed()));
        }
    }
}
