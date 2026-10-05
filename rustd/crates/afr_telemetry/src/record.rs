//! Where every runner measurement is recorded: one trait, one process-wide
//! slot, and free functions a producer calls.
//!
//! # Why a trait object in a slot
//!
//! The producers live in four crates and none of them should know whether
//! telemetry is on. They call [`turn`], [`retry`] and the rest; each reaches
//! the [`Recorder`] `run` installed, or nothing. That is the daemon's shape
//! (`afd_observability::producers`) with one change: the slot holds a trait
//! object rather than a struct of handles, so a test can route one future's
//! measurements to a recorder of its own (`testing::scoped`) without a
//! process-wide install racing every other test in the binary — the way
//! `tracing` routes events with a scoped dispatcher.
//!
//! # Nothing installed is not an error
//!
//! A runner with no endpoint never installs a recorder, and every function
//! here is then a load of an empty slot and a return.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::labels::{
    FrameDrop, Provider, PushFailure, RetryReason, SandboxStart, Tool, ToolOutcome, TurnOutcome,
};

/// What every runner measurement is recorded through.
///
/// One method per family, each taking only its closed label set, so a call
/// site cannot attribute a measurement to a label the census never declared.
pub trait Recorder: Send + Sync + core::fmt::Debug {
    /// One model turn ended, after `elapsed`.
    fn turn(&self, provider: Provider, outcome: TurnOutcome, elapsed: Duration);
    /// A turn's send is about to be retried.
    fn retry(&self, provider: Provider, reason: RetryReason);
    /// A sandbox start ended, after `elapsed`.
    fn sandbox_start(&self, outcome: SandboxStart, elapsed: Duration);
    /// `frames` live-tail frames never left the runner.
    fn frames_dropped(&self, reason: FrameDrop, frames: u64);
    /// A memory push did not land.
    fn push_failed(&self, reason: PushFailure);
    /// One tool call ended, after `elapsed`.
    fn tool_call(&self, tool: Tool, outcome: ToolOutcome, elapsed: Duration);
    /// The span budget shed `spans`.
    fn spans_suppressed(&self, spans: u64);
}

/// The recorder `run` installed, once per process.
static INSTALLED: OnceLock<Arc<dyn Recorder>> = OnceLock::new();

/// Installs `recorder` for the rest of the process.
///
/// Answers whether it took: the first install wins, because one process
/// exports through one pipeline and a second would be a second set of series
/// under the same names.
pub fn install(recorder: Arc<dyn Recorder>) -> bool {
    INSTALLED.set(recorder).is_ok()
}

/// Runs `record` against the recorder in force here, if there is one.
fn with(record: impl FnOnce(&dyn Recorder)) {
    #[cfg(feature = "test-util")]
    if let Some(scoped) = crate::testing::scoped_recorder() {
        record(scoped.as_ref());
        return;
    }
    if let Some(recorder) = INSTALLED.get() {
        record(recorder.as_ref());
    }
}

/// Records one model turn.
pub fn turn(provider: Provider, outcome: TurnOutcome, elapsed: Duration) {
    with(|recorder| recorder.turn(provider, outcome, elapsed));
}

/// Records a turn's send about to be retried.
pub fn retry(provider: Provider, reason: RetryReason) {
    with(|recorder| recorder.retry(provider, reason));
}

/// Records a sandbox start.
pub fn sandbox_start(outcome: SandboxStart, elapsed: Duration) {
    with(|recorder| recorder.sandbox_start(outcome, elapsed));
}

/// Records live-tail frames lost on the runner.
pub fn frames_dropped(reason: FrameDrop, frames: u64) {
    with(|recorder| recorder.frames_dropped(reason, frames));
}

/// Records a memory push that did not land.
pub fn push_failed(reason: PushFailure) {
    with(|recorder| recorder.push_failed(reason));
}

/// Records one tool call.
pub fn tool_call(tool: Tool, outcome: ToolOutcome, elapsed: Duration) {
    with(|recorder| recorder.tool_call(tool, outcome, elapsed));
}

/// Records spans the budget shed.
pub fn spans_suppressed(spans: u64) {
    with(|recorder| recorder.spans_suppressed(spans));
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    use std::time::Duration;

    use crate::labels::{
        FrameDrop, Provider, PushFailure, RetryReason, SandboxStart, Tool, ToolOutcome, TurnOutcome,
    };
    use crate::testing::{Recorded, Tally, scoped};

    /// A scoped future's measurements reach its own recorder, each through
    /// the free function its producer calls; outside the scope they do not.
    #[tokio::test]
    async fn a_scoped_future_records_into_its_own_recorder() {
        let (tally, received) = Tally::new();
        let elapsed = Duration::from_millis(3);
        let anthropic = Provider::of("anthropic");
        let tool = Tool::of("file_read");

        scoped(tally, async {
            super::turn(anthropic, TurnOutcome::Completed, elapsed);
            super::retry(anthropic, RetryReason::Transport);
            super::sandbox_start(SandboxStart::Failed, elapsed);
            super::frames_dropped(FrameDrop::PostFailed, 4);
            super::push_failed(PushFailure::Refused);
            super::tool_call(tool, ToolOutcome::Interrupted, elapsed);
            super::spans_suppressed(2);
        })
        .await;
        super::push_failed(PushFailure::Internal);

        let recorded: Vec<Recorded> = received.try_iter().collect();
        assert_eq!(
            recorded,
            vec![
                Recorded::Turn(anthropic, TurnOutcome::Completed, elapsed),
                Recorded::Retry(anthropic, RetryReason::Transport),
                Recorded::SandboxStart(SandboxStart::Failed, elapsed),
                Recorded::FramesDropped(FrameDrop::PostFailed, 4),
                Recorded::PushFailed(PushFailure::Refused),
                Recorded::ToolCall(tool, ToolOutcome::Interrupted, elapsed),
                Recorded::SpansSuppressed(2),
            ],
            "everything inside the scope, and the push after it went elsewhere"
        );
    }
}
