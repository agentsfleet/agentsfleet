//! The run's streamed text, as live frames. Each tool call's frames are the
//! [`Ledger`](crate::ledger::Ledger)'s.
//!
//! Answer and reasoning text share one run-wide `stream_seq` from 0, the first
//! chunk alone carries `stream_start` and its time since the run started, and
//! each kind keeps its own scrub carry so a secret split across chunks is never
//! sent (`docs/architecture/runner_fleet.md` §"Live activity (the SSE tail)").

use std::borrow::Cow;
use std::time::Instant;

use afd_core::clock::saturating_millis;
use afd_wire::activity::{ActivityFrame, FleetResponseChunk, StreamTextKind};

use crate::engine::EventSink;
use afr_secrets::{Carry, Scrub};

/// Frames that go nowhere: a child loop's text is its parent's to read
/// through the call that started it, never the thread's answer.
static SILENT: Silent = Silent;

/// A sink that drops every frame.
struct Silent;

impl EventSink for Silent {
    fn emit(&self, _frame: ActivityFrame<'static>) {}
}

/// Where one run's frames go, and the stream's position.
pub(crate) struct Live<'run> {
    sink: &'run dyn EventSink,
    scrub: &'run Scrub,
    started: Instant,
    next_seq: u64,
    answer: Carry,
    reasoning: Carry,
}

impl<'run> Live<'run> {
    /// Frames for a run that started at `started`.
    pub(crate) fn new(sink: &'run dyn EventSink, scrub: &'run Scrub, started: Instant) -> Self {
        Self {
            sink,
            scrub,
            started,
            next_seq: 0,
            answer: Carry::default(),
            reasoning: Carry::default(),
        }
    }

    /// Frames for a child loop, which sends none.
    pub(crate) fn silent(scrub: &'run Scrub, started: Instant) -> Self {
        Self::new(&SILENT, scrub, started)
    }

    /// Streams `text` of `kind`, holding back any tail that could still be a
    /// secret's start.
    pub(crate) fn text(&mut self, kind: StreamTextKind, text: &str) {
        let carry = match kind {
            StreamTextKind::Answer => &mut self.answer,
            StreamTextKind::Reasoning => &mut self.reasoning,
        };
        let ready = carry.push(self.scrub, text);
        if ready.is_empty() {
            return;
        }
        let first = self.next_seq == 0;
        let first_chunk_after_ms = first.then(|| saturating_millis(self.started.elapsed()));
        self.sink
            .emit(ActivityFrame::FleetResponseChunk(FleetResponseChunk {
                text: Cow::Owned(ready),
                text_kind: Some(kind),
                first_chunk_after_ms,
                stream_start: first,
                stream_contiguous: true,
                stream_seq: self.next_seq,
            }));
        self.next_seq += 1;
    }

    /// Ends a model pass: a held tail is dropped, never sent, because it may
    /// be the head of a secret. The report carries the whole answer.
    pub(crate) fn end_pass(&mut self) {
        self.answer = Carry::default();
        self.reasoning = Carry::default();
    }
}

#[cfg(test)]
#[path = "events/tests.rs"]
mod tests;
