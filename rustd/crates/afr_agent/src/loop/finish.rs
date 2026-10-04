//! A run's end: the result the report carries, and what the run leaves behind.

use afd_wire::report::{Completed, ExecutionResult, Failure, ResultOutcome};

use super::{Ending, Harness};
use crate::engine::RunOutput;

/// What a run stopped by its lease reports as its detail.
pub(super) const DETAIL_STOPPED: &str = "the run was stopped before it finished";

impl Harness<'_> {
    /// The outcome, the answer and the tokens spent, with the calls and the
    /// memory the run leaves for the supervisor to post.
    pub(super) fn finish(self, ending: Ending) -> RunOutput {
        let (outcome, content) = match ending {
            Ending::Answered(text) => (
                ResultOutcome::Completed(Completed {}),
                self.scrub.text(&text).into_owned(),
            ),
            Ending::Failed(failure) => {
                let failed = Failure {
                    class: failure.failure_class(),
                    detail: failure.detail().into(),
                };
                (ResultOutcome::Failed(failed), String::new())
            }
            Ending::Stopped => {
                let stopped = Failure {
                    class: None,
                    detail: DETAIL_STOPPED.into(),
                };
                (ResultOutcome::Failed(stopped), String::new())
            }
        };
        let usage = self.meter.read();
        let (trace, records) = self.ledger.finish();
        RunOutput {
            result: ExecutionResult {
                outcome,
                content: content.into(),
                token_count: usage.total(),
                wall_seconds: self.started.elapsed().as_secs(),
                memory_peak_bytes: 0,
                cpu_throttled_ms: 0,
                input_tokens: usage.input,
                cached_input_tokens: usage.cached_input,
                output_tokens: usage.output,
            },
            memory: self.lease.memory.into_pending(),
            trace,
            records,
        }
    }
}
