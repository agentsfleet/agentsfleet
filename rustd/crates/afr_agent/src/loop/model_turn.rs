//! One model turn: the request the loop sends, the stream it takes, and the
//! start and end it logs and counts.

use std::time::Instant;

use afr_providers::Request;
use afr_telemetry::labels::TurnOutcome;
use afr_telemetry::record;
use tracing::Instrument as _;

use super::{
    EVENT_PROVIDER_FAILED, EVENT_TURN_COMPLETED, EVENT_TURN_STARTED, Harness, REASON_STOPPED,
};
use crate::spans;
use crate::turn::{Turn, take};

impl Harness<'_, '_> {
    /// One model turn, its start and its end logged as a pair
    /// (`docs/LOGGING_STANDARD.md` §4 rule 1, at `debug` because a run makes
    /// one per pass); `None` when the loop was stopped.
    pub(super) async fn turn(
        &mut self,
        number: u64,
        capped: bool,
    ) -> Option<afr_providers::Result<Turn>> {
        let shared = self.shared;
        let lease_id = shared.lease_id;
        let depth = self.depth;
        let turn = number;
        let event = EVENT_TURN_STARTED;
        tracing::debug!(lease_id, depth, turn, event);
        let request = Request {
            model: shared.model,
            instructions: &shared.instructions,
            messages: &self.messages,
            tools: if capped { &[] } else { &self.specs },
            hosted: if capped { &[] } else { &self.hosted },
        };
        let span = spans::chat(shared.model);
        let started = Instant::now();
        let streamed =
            take(shared.provider.stream(request), &mut self.live).instrument(span.clone());
        let taken = tokio::select! {
            biased;
            () = self.stop.cancelled() => None,
            taken = streamed => Some(taken),
        };
        record::turn(
            shared.provider_label,
            turn_outcome(taken.as_ref()),
            started.elapsed(),
        );
        match &taken {
            Some(Ok(done)) => {
                let input_tokens = done.usage.prompt();
                let output_tokens = done.usage.output;
                spans::spent(&span, input_tokens, output_tokens);
                let calls = done.calls.len();
                let event = EVENT_TURN_COMPLETED;
                tracing::debug!(
                    lease_id,
                    depth,
                    turn,
                    input_tokens,
                    output_tokens,
                    calls,
                    event
                );
            }
            Some(Err(failure)) => {
                let code = failure.code().as_str();
                let event = EVENT_PROVIDER_FAILED;
                tracing::warn!(error_code = code, lease_id, depth, turn, event);
            }
            None => {
                let reason = REASON_STOPPED;
                let event = EVENT_PROVIDER_FAILED;
                tracing::debug!(lease_id, depth, turn, reason, event);
            }
        }
        taken
    }
}

/// How a turn ended, as the turn-duration family labels it.
const fn turn_outcome(taken: Option<&afr_providers::Result<Turn>>) -> TurnOutcome {
    match taken {
        Some(Ok(_answered)) => TurnOutcome::Completed,
        Some(Err(_failed)) => TurnOutcome::Failed,
        None => TurnOutcome::Stopped,
    }
}
