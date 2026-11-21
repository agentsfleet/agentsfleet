//! The run's memory, written by the report that carries it.
//!
//! ```text
//!   load (this runner's run) ─► capture memory ─► commit the settle ─► publish
//!                                  └ fenced; a failure is logged, the report settles
//! ```
//!
//! Before the commit, because the commit frees the fleet's thread: a run for
//! the fleet cannot issue until then, so the next run's window reads what
//! this run learned. In its own transaction under the memory role, because the
//! settle's runs under the run role and a run store that could write
//! memory would need both. An idempotent upsert, so a report the fence then
//! refuses, or a retried one, writes the same rows again.
//!
//! A capture that fails does not refuse the report, the decision the runner's
//! own push made: withholding the answer would lose it too and let the run
//! lapse into a re-run.

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_wire::report::ReportRequest;
use jiff::Timestamp;

use crate::run::memory::Holder;
use crate::run::pull::RunService;
use crate::run::settle::Reported;

/// The report's memory could not be written; the report still settles.
pub(super) const EVENT_CAPTURE_FAILED: &str = "memory_capture_failed";

/// The report's run was no longer live, so its memory was not written again.
const EVENT_CAPTURE_SKIPPED: &str = "memory_capture_skipped";

impl RunService {
    /// Writes the report's memory deltas, fenced on the run's own token.
    /// A fence refusal was logged by the fence; anything else is logged here.
    pub(super) async fn capture_reported(
        &self,
        runner_id: &Uuid7,
        run: &Reported,
        request: &ReportRequest<'_>,
        now: Timestamp,
    ) {
        if request.memory.is_empty() {
            return;
        }
        // The row's token, as the settle is fenced on it: the report is this
        // runner's by the load, and a run the fleet has moved past writes
        // neither its memory nor its settle.
        // The report was loaded by this id, so it parses; a value that does
        // not names no run and has no memory to write.
        let Ok(run_key) = Uuid7::parse(request.run_id.as_ref()) else {
            return;
        };
        let holder = Holder {
            runner_id,
            fleet_id: &run.fleet_id,
            run_id: &run_key,
            fencing_token: run.fence.as_u64(),
        };
        // Untimed: this capture is the report's own `capture` stage.
        let mut untimed = af_observability::verb_span::VerbClock::untimed();
        let Err(failure) = self
            .capture_for(holder, &request.memory, now, &mut untimed)
            .await
        else {
            return;
        };
        if failure.is_fence_refusal() {
            return;
        }
        // A run no longer live: a repeated report, whose first post already
        // wrote this memory, or one the settle will refuse in a moment.
        if failure.code() == af_core::error_code::RUN_NOT_FOUND {
            let fleet_id = run.fleet_id.as_str();
            let run_id = request.run_id.as_ref();
            let event = EVENT_CAPTURE_SKIPPED;
            tracing::debug!(fleet_id, run_id, event);
            return;
        }
        let Logged { error_code, reason } = failure.logged();
        let fleet_id = run.fleet_id.as_str();
        let run_id = request.run_id.as_ref();
        let event = EVENT_CAPTURE_FAILED;
        {
            let (message,) = (format_args!(
                "the run's memory was not written; the report still settles"
            ),);
            tracing::warn!(message, error_code, fleet_id, run_id, reason, event);
        };
    }
}
