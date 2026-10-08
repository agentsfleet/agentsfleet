//! A run's end: the sessions its calls left open, the result the report
//! carries, and what the run leaves behind.

use crate::{Completed, ExecutionResult, Failure, ResultOutcome};
use afd_wire::memory::MemoryDelta;

use super::{Ending, Harness};
use crate::engine::RunOutput;

/// What a run stopped by its lease reports as its detail.
pub(super) const DETAIL_STOPPED: &str = "the run was stopped before it finished";
/// The event a run's end logs when it killed sessions still open.
pub(super) const EVENT_SESSIONS_INTERRUPTED: &str = "sessions_interrupted";

impl Harness<'_, '_> {
    /// Closes every session the run's calls left open, then hands back the
    /// outcome, the answer and the tokens spent, with the calls and the memory
    /// the run leaves for the supervisor to post. The root's end; a child
    /// concludes instead.
    pub(super) async fn finish(self, ending: Ending) -> RunOutput {
        self.close_sessions().await;
        let shared = self.shared;
        let (outcome, content) = match ending {
            Ending::Answered(text) => (
                ResultOutcome::Completed(Completed),
                shared.scrub.text(&text).into_owned(),
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
        let usage = shared.meter.read();
        let (trace, records) = shared.ledger.finish();
        let memory = (shared.lease.memory.lock().await)
            .pending()
            .into_iter()
            .map(MemoryDelta::into_owned)
            .collect();
        RunOutput {
            result: ExecutionResult {
                outcome,
                content: content.into(),
                token_count: usage.total(),
                wall_seconds: shared.started.elapsed().as_secs(),
                memory_peak_bytes: 0,
                cpu_throttled_ms: 0,
                input_tokens: usage.input,
                cached_input_tokens: usage.cached_input,
                output_tokens: usage.output,
            },
            memory,
            trace,
            records,
        }
    }

    /// Kills every process a session still holds, so none outlives the run
    /// whatever becomes of its sandbox, and logs how many there were. A call
    /// the lease stopped mid-yield left its process registered, so it is
    /// closed here too.
    async fn close_sessions(&self) {
        let Some(executor) = self.shared.executor else {
            return;
        };
        let sessions = self.shared.lease.sessions.close_all(executor).await;
        if sessions > 0 {
            let lease_id = self.shared.lease_id;
            let event = EVENT_SESSIONS_INTERRUPTED;
            tracing::info!(lease_id, sessions, event);
        }
    }
}
