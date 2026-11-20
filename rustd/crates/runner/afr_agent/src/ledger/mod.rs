//! The ledger: what each tool call did, kept in one place.
//!
//! `guffbolt` opens, runs and ends each call itself, the shape Exonum's
//! `TopLevelContext::call` gives a transaction: one wrapper guarantees one
//! outcome. The ledger hears both ends. [`Ledger::open`] numbers a call from
//! 1 and sends its `tool_call_started` frame. [`Ledger::close`] sends its
//! `tool_call_completed` frame, adds its trace row and, when it answered,
//! holds its full record. A call whose run was dropped before it answered
//! (the run stopped, the run future was dropped) ends `interrupted`, live
//! and in the trace, so no run ending can leave a call open. Records are held
//! only while they fit what one event may keep, measured as the daemon
//! measures it: one it would refuse is not held to be posted.
//!
//! Each record names the turn newest when its call's step began, the mark
//! `guffbolt` ends the call with; a call whose step carried none, a child's,
//! names the newest the run heard, and the run's own turn before any. An
//! edit's record, its trace row and its end frame carry the files it changed,
//! read from the report its file tool ended its output with (`changes`).

use std::borrow::Cow;
use std::sync::{Mutex, MutexGuard, PoisonError};

use af_core::clock::{saturating_millis, saturating_millis_signed};
use af_wire::activity::{ActivityFrame, ToolCallCompleted, ToolCallStarted};
use af_wire::tool_detail::{DETAIL_EVENT_MAX_BYTES, ToolCallRecord};
use af_wire::tool_trace::{ToolCallStatus, ToolTrace};
use afr_telemetry::labels::{Tool, ToolOutcome};
use afr_telemetry::record;
use afr_tools::ToolErrorCode;
use guffbolt::{Call, CallEnd, CallOutcome, Mark};
use serde_json::{Map, Value};
use tracing::Span;

use crate::engine::EventSink;
use crate::records::{Item, record};
use crate::spans;
use crate::trace::{Outcome, Trace, bounded_arguments};
use af_secrets::{Clean, Scrub};

pub(crate) const EVENT_CALL_STARTED: &str = "tool_call_started";
pub(crate) const EVENT_CALL_COMPLETED: &str = "tool_call_completed";
/// A full record past the event's budget, kept out of the post. The trace
/// still lists the call, so an operator's "show all" finds no record for it.
const EVENT_RECORD_DROPPED: &str = "tool_record_dropped";

/// Every call one run made, its children's included.
///
/// Shared by the run's loop and every child loop it starts, so their calls
/// take one counter and land in one trace. What a call changes sits behind
/// one lock, taken to open the call and again to end it, never while it
/// runs: a child's call opens and ends while its parent's `delegate` call is
/// still open.
pub(crate) struct Ledger<'run> {
    run_id: &'run str,
    /// The turn the run was assigned for.
    turn_id: &'run str,
    sink: &'run dyn EventSink,
    scrub: &'run Scrub,
    book: Mutex<Book>,
}

/// What the run's calls have left so far.
#[derive(Default)]
struct Book {
    trace: Trace,
    records: Vec<ToolCallRecord<'static>>,
    /// What the held records spend of the event's budget.
    spent: usize,
    calls: u64,
    /// The newest turn a call's step began with, once one did.
    newest: Option<String>,
}

impl<'run> Ledger<'run> {
    /// Run `run_id`'s ledger, assigned for `turn_id`, sending its frames to
    /// `sink` and masking through `scrub`.
    pub(crate) fn new(
        run_id: &'run str,
        turn_id: &'run str,
        sink: &'run dyn EventSink,
        scrub: &'run Scrub,
    ) -> Self {
        Self {
            run_id,
            turn_id,
            sink,
            scrub,
            book: Mutex::default(),
        }
    }

    /// Opens `call` as the run's next: numbers it, scrubs and bounds its
    /// arguments, and sends its start frame. Hands back what its end needs,
    /// and the span it runs in.
    pub(crate) fn open(&self, call: &Call) -> (Opened, Span) {
        // Numbered and announced under the lock, so the start frames leave
        // in the order the calls were numbered.
        let mut book = self.book();
        book.calls += 1;
        let number = book.calls;
        let id = number.to_string();
        let shown = self.scrub.clean_json(call.arguments.clone());
        let bounded = bounded_arguments(&shown);
        // The frame's field is JSON by its wire definition, so a debug-form
        // fallback would break it; a JSON value always encodes, so there is
        // no failure to fall back from or carry.
        let args_redacted = Value::Object(bounded.clone()).to_string();
        let run_id = self.run_id;
        let call_id = id.as_str();
        let tool = call.name.as_str();
        let event = EVENT_CALL_STARTED;
        tracing::debug!(run_id, call_id, tool, event);
        self.sink
            .emit(ActivityFrame::ToolCallStarted(ToolCallStarted {
                name: Cow::Owned(call.name.clone()),
                args_redacted: Cow::Owned(args_redacted),
                call_id: Cow::Owned(id.clone()),
            }));
        drop(book);
        let span = spans::execute_tool(&call.name, &id);
        let opened = Opened {
            number,
            id,
            shown,
            bounded,
        };
        (opened, span)
    }

    /// Ends the call `opened` holds, as `end` says. A call that answered has
    /// its error code typed on its span and its output masked in place, so
    /// the model reads what the frames and the trace show.
    pub(crate) fn close(&self, opened: Opened, end: CallEnd<'_>) {
        let status = status(end.outcome());
        let Opened {
            number,
            id,
            shown,
            bounded,
        } = opened;
        let ended = Ended {
            number,
            id,
            name: &end.call.name,
        };
        let Some(output) = end.output else {
            let outcome = Outcome::interrupted(end.duration);
            self.end(&mut self.book(), ended, bounded, outcome);
            return;
        };
        spans::failed(end.span, output.error_code);
        if output.error_code == Some(ToolErrorCode::OutOfMemory.into()) {
            record::out_of_memory();
        }
        let text = self.scrub.clean(std::mem::take(&mut output.text));
        let mut outcome = Outcome::ended(status, &text, output.exit_code, end.duration);
        let kind = changes::kind(ended.name);
        let changed = changes::changed(self.run_id, kind, status, &text);
        outcome.changes.clone_from(&changed);
        let item = Item {
            turn_id: self.turn(end.mark),
            kind,
            tool: ended.name.to_owned(),
            status: changes::item_status(status),
            changes: changed,
        };
        let full = record(number, shown, &text, item);
        output.text = text.into_inner();
        let mut book = self.book();
        self.end(&mut book, ended, bounded, outcome);
        self.hold(&mut book, full);
    }

    /// The turn a call's item names: `mark`, the newest when its step began,
    /// which the run has then heard; the newest the run heard for a call
    /// whose step carried none; the run's own before any.
    fn turn(&self, mark: Option<&Mark>) -> String {
        let mut book = self.book();
        if let Some(mark) = mark {
            book.newest = Some(mark.as_str().to_owned());
        }
        book.newest
            .clone()
            .unwrap_or_else(|| self.turn_id.to_owned())
    }

    /// Logs a call's end, counts it, sends its end frame and adds its trace
    /// row.
    fn end(
        &self,
        book: &mut Book,
        ended: Ended<'_>,
        bounded: Map<String, Value>,
        outcome: Outcome,
    ) {
        let run_id = self.run_id;
        let call_id = ended.id.as_str();
        let tool = ended.name;
        let status = outcome.status;
        let duration_ms = saturating_millis(outcome.elapsed);
        let event = EVENT_CALL_COMPLETED;
        tracing::debug!(run_id, call_id, tool, ?status, duration_ms, event);
        record::tool_call(Tool::of(tool), tool_outcome(status), outcome.elapsed);
        self.sink
            .emit(ActivityFrame::ToolCallCompleted(ToolCallCompleted {
                name: Cow::Owned(tool.to_owned()),
                ms: saturating_millis_signed(outcome.elapsed),
                call_id: Cow::Owned(ended.id),
                status: outcome.status,
                output_head: outcome.head.clone().map(Cow::Owned),
                output_tail: outcome.tail.clone().map(Cow::Owned),
                output_line_count: outcome.line_count,
                exit_code: outcome.exit_code,
                changes: outcome.changes.clone(),
            }));
        book.trace.push(ended.number, tool, bounded, outcome);
    }

    /// Holds `record` when it fits what is left of [`DETAIL_EVENT_MAX_BYTES`].
    /// A record past it is dropped and a later, smaller one may still fit, as
    /// the daemon keeps them.
    fn hold(&self, book: &mut Book, record: ToolCallRecord<'static>) {
        let after = book.spent.saturating_add(record.byte_count());
        if after <= DETAIL_EVENT_MAX_BYTES {
            book.spent = after;
            book.records.push(record);
        } else {
            let run_id = self.run_id;
            let call_number = record.call_number;
            let bytes = record.byte_count();
            let event = EVENT_RECORD_DROPPED;
            tracing::warn!(run_id, call_number, bytes, event);
        }
    }

    /// The run's known trace, including zero calls, and every record.
    pub(crate) fn finish(&self) -> (ToolTrace<'static>, Vec<ToolCallRecord<'static>>) {
        let book = std::mem::take(&mut *self.book());
        (book.trace.finish(), book.records)
    }

    /// The book, whatever a panicking holder left: a call's end is one row
    /// pushed and one record held, so a panic leaves it whole.
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One call between its start frame and its end: what its trace row and its
/// record are built from.
pub(crate) struct Opened {
    number: u64,
    id: String,
    /// The scrubbed arguments, whole, for the record.
    shown: Clean<Value>,
    /// The same arguments within the trace row's bounds.
    bounded: Map<String, Value>,
}

/// Which call ended, as its end frame and its trace row name it.
struct Ended<'a> {
    number: u64,
    id: String,
    name: &'a str,
}

/// How a call ended, as its frames and its trace row carry it.
const fn status(outcome: CallOutcome) -> ToolCallStatus {
    match outcome {
        CallOutcome::Succeeded => ToolCallStatus::Succeeded,
        CallOutcome::Failed => ToolCallStatus::Failed,
        CallOutcome::Interrupted => ToolCallStatus::Interrupted,
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

mod changes;

#[cfg(test)]
mod item_tests;
#[cfg(test)]
mod test_calls;
#[cfg(test)]
mod tests;
