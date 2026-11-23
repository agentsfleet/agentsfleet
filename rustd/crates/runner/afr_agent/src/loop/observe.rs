//! One loop's work as the run records it, heard from `guffbolt`: each model
//! turn's logs, count and span, its text sent live and its spend metered,
//! and each tool call kept in the run's ledger.

use afr_telemetry::labels::TurnOutcome as TurnLabel;
use afr_telemetry::record;
use guffbolt::{Call, CallEnd, Listen, Observe, StepEnd, StepOutcome, TextKind, Usage};
use tracing::Span;

use super::{
    EVENT_CHILD_ENDED, EVENT_PROVIDER_FAILED, EVENT_TURN_COMPLETED, EVENT_TURN_STARTED, Ending,
    Harness, REASON_STOPPED,
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

    fn step_started(&self, number: u64) -> Span {
        let shared = self.shared;
        let run_id = shared.run_id;
        let depth = self.depth;
        let event = EVENT_TURN_STARTED;
        tracing::debug!(run_id, depth, turn = number, event);
        spans::chat(shared.model)
    }

    fn step_ended(&self, end: StepEnd<'_, af_providers::Error>) {
        let shared = self.shared;
        record::turn(shared.provider_label, label(&end.outcome), end.duration);
        let run_id = shared.run_id;
        let depth = self.depth;
        let number = end.number;
        match end.outcome {
            StepOutcome::Read(done) => {
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
            StepOutcome::Failed(failure) => {
                let code = failure.code().as_str();
                let event = EVENT_PROVIDER_FAILED;
                tracing::warn!(error_code = code, run_id, depth, turn = number, event);
            }
            StepOutcome::Stopped => {
                let reason = REASON_STOPPED;
                let event = EVENT_PROVIDER_FAILED;
                tracing::debug!(run_id, depth, turn = number, reason, event);
            }
        }
    }

    fn call_started(&self, call: &Call) -> (Opened, Span) {
        self.shared.ledger.open(call)
    }

    fn call_ended(&self, kept: Opened, end: CallEnd<'_>) {
        self.shared.ledger.close(kept, end);
    }

    /// A child's turn ends here, paired with its start
    /// ([`Harness::child`]); the root's end is the run's report.
    fn loop_ended(&self, steps: u64, ending: &Ending) {
        if self.depth == 0 {
            return;
        }
        let run_id = self.shared.run_id;
        let child_id = self.id;
        let depth = self.depth;
        let status = ending.kind();
        let event = EVENT_CHILD_ENDED;
        tracing::info!(run_id, child_id, depth, status, steps, event);
    }
}

/// How a turn ended, as the turn-duration family labels it.
const fn label(outcome: &StepOutcome<'_, af_providers::Error>) -> TurnLabel {
    match outcome {
        StepOutcome::Read(_answered) => TurnLabel::Completed,
        StepOutcome::Failed(_failed) => TurnLabel::Failed,
        StepOutcome::Stopped => TurnLabel::Stopped,
    }
}
