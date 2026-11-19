//! One loop's work as the run records it, heard from `guffbolt`: each model
//! turn's logs, count and span, its text sent live and its spend metered,
//! and each tool call kept in the run's ledger.

use afr_telemetry::labels::TurnOutcome as TurnLabel;
use afr_telemetry::record;
use guffbolt::{Call, CallEnd, Listen, Observe, TextKind, TurnEnd, TurnOutcome, Usage};
use tracing::Span;

use super::{
    EVENT_PROVIDER_FAILED, EVENT_TURN_COMPLETED, EVENT_TURN_STARTED, Harness, REASON_STOPPED,
};
use crate::ledger::Opened;
use crate::spans;

/// What a turn streams reaches this loop's frames as it arrives, and what it
/// spends reaches the run's meter, so a turn cut short still has both.
impl Listen for Harness<'_, '_> {
    fn text(&self, kind: TextKind, text: &str) {
        self.live.text(kind, text);
    }

    fn usage(&self, usage: Usage) {
        self.shared.meter.add(usage);
    }

    fn end(&self) {
        self.live.end();
    }
}

/// Each model turn's start and end are logged as a pair (LOG-PAIR and
/// LOG-HOT, `.orly/standards/domains/logging.md`, at `debug` because a run
/// makes one per pass), counted by how it ended, and traced in a `chat` span.
/// Each tool call opens and ends in the run's one ledger, so a child's calls
/// take the same counter as its parent's.
impl Observe<af_providers::Error> for Harness<'_, '_> {
    type Call = Opened;

    fn turn_started(&self, number: u64) -> Span {
        let shared = self.shared;
        let run_id = shared.run_id;
        let depth = self.depth;
        let event = EVENT_TURN_STARTED;
        tracing::debug!(run_id, depth, turn = number, event);
        spans::chat(shared.model)
    }

    fn turn_ended(&self, end: TurnEnd<'_, af_providers::Error>) {
        let shared = self.shared;
        record::turn(shared.provider_label, label(&end.outcome), end.duration);
        let run_id = shared.run_id;
        let depth = self.depth;
        let number = end.number;
        match end.outcome {
            TurnOutcome::Read(done) => {
                let input_tokens = done.usage.prompt();
                let output_tokens = done.usage.output;
                spans::spent(end.span, done.usage);
                let calls = done.calls.len();
                let event = EVENT_TURN_COMPLETED;
                tracing::debug!(
                    run_id,
                    depth,
                    turn = number,
                    input_tokens,
                    output_tokens,
                    calls,
                    event
                );
            }
            TurnOutcome::Failed(failure) => {
                let code = failure.code().as_str();
                let event = EVENT_PROVIDER_FAILED;
                tracing::warn!(error_code = code, run_id, depth, turn = number, event);
            }
            TurnOutcome::Stopped => {
                let reason = REASON_STOPPED;
                let event = EVENT_PROVIDER_FAILED;
                tracing::debug!(run_id, depth, turn = number, reason, event);
            }
        }
    }

    fn call_started(&self, call: &Call) -> (Opened, Span) {
        self.shared.ledger.open(call)
    }

    /// A child's call that answered counts toward what its parent reads of
    /// it; one interrupted does not.
    fn call_ended(&self, kept: Opened, end: CallEnd<'_>) {
        let answered = end.output.is_some();
        self.shared.ledger.close(kept, end);
        if answered && let Some(child) = &self.child {
            child.called();
        }
    }
}

/// How a turn ended, as the turn-duration family labels it.
const fn label(outcome: &TurnOutcome<'_, af_providers::Error>) -> TurnLabel {
    match outcome {
        TurnOutcome::Read(_answered) => TurnLabel::Completed,
        TurnOutcome::Failed(_failed) => TurnLabel::Failed,
        TurnOutcome::Stopped => TurnLabel::Stopped,
    }
}
