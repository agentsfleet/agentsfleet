//! Where a lease's time went, logged on the line that ends it.
//!
//! ```text
//!   bundle ─► memory ─► sandbox ─► model + tools ─► report
//!   └── each once, absent when it never started; wall_ms is the whole lease ──┘
//! ```
//!
//! Six segments tile the lease: the bundle fetch, the memory hydrate, the
//! sandbox from its sizing to its landing, the model's turns, the tools those
//! turns called, and the settle from hold-or-destroy to the posted report.
//! What they leave of the wall time is the admission, the turn claim and the
//! drain wait. The first token is inside the model's time and is logged beside
//! them. A segment a lease never reached is absent from the line, never zero:
//! an operator reading `sandbox_ms=0` would take it for a held sandbox.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use afd_core::clock::saturating_millis;
use afd_core::spelling::to_spelling;
use tokio::time::Instant;

use super::{EVENT_COMPLETED, EVENT_FAILED, LeaseRun};
use crate::activity::RunTiming;
use crate::report::Ending;

/// The segments a stopwatch times; the model and its tools are read off the
/// run's frames instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Segment {
    Bundle,
    Memory,
    Sandbox,
    Report,
}

/// How long each segment took, for those that ran.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Segments {
    bundle: Option<Duration>,
    memory: Option<Duration>,
    sandbox: Option<Duration>,
    model: Option<Duration>,
    tools: Option<Duration>,
    report: Option<Duration>,
    first_token: Option<Duration>,
}

/// A lease's segments as they finish, written from whichever step ends one.
#[derive(Debug, Default)]
pub(super) struct Timeline {
    segments: Mutex<Segments>,
}

impl Timeline {
    /// Starts timing `segment`; the watch records it when stopped or dropped.
    pub(super) fn start(&self, segment: Segment) -> Stopwatch<'_> {
        Stopwatch {
            timeline: self,
            segment,
            started: Instant::now(),
            recorded: false,
        }
    }

    /// The run's own timing: the model's time, its tools' and its first token.
    pub(super) fn ran(&self, timing: RunTiming) {
        let mut segments = self.lock();
        segments.model = Some(timing.model());
        segments.tools = Some(timing.tools);
        segments.first_token = timing.first_chunk;
    }

    fn set(&self, segment: Segment, elapsed: Duration) {
        let mut segments = self.lock();
        let slot = match segment {
            Segment::Bundle => &mut segments.bundle,
            Segment::Memory => &mut segments.memory,
            Segment::Sandbox => &mut segments.sandbox,
            Segment::Report => &mut segments.report,
        };
        *slot = Some(elapsed);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Segments> {
        // Nothing panics while a segment is written, so the lock is never
        // poisoned; a poisoned one would still carry every segment written.
        self.segments.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One segment being timed. Dropping it records the time so far, so a step
/// that returns early still leaves its segment on the line.
#[derive(Debug)]
pub(super) struct Stopwatch<'a> {
    timeline: &'a Timeline,
    segment: Segment,
    started: Instant,
    recorded: bool,
}

impl Stopwatch<'_> {
    /// The segment ends here.
    pub(super) fn stop(mut self) {
        self.record();
    }

    fn record(&mut self) {
        if !self.recorded {
            self.recorded = true;
            self.timeline.set(self.segment, self.started.elapsed());
        }
    }
}

impl Drop for Stopwatch<'_> {
    fn drop(&mut self) {
        self.record();
    }
}

impl LeaseRun<'_> {
    /// Runs `work` as `segment`, whichever way it ends.
    pub(super) async fn timed<T>(&self, segment: Segment, work: impl Future<Output = T>) -> T {
        let watch = self.timeline.start(segment);
        let out = work.await;
        watch.stop();
        out
    }

    /// Logs how the lease ended and where its time went.
    pub(super) fn conclude(&self, ending: &Ending) {
        let lease_id = self.ids.lease.as_str();
        let wall_ms = saturating_millis(self.started.elapsed());
        let segments = *self.timeline.lock();
        let bundle_ms = segments.bundle.map(saturating_millis);
        let memory_ms = segments.memory.map(saturating_millis);
        let sandbox_ms = segments.sandbox.map(saturating_millis);
        let model_ms = segments.model.map(saturating_millis);
        let tools_ms = segments.tools.map(saturating_millis);
        let report_ms = segments.report.map(saturating_millis);
        let first_token_ms = segments.first_token.map(saturating_millis);
        match ending {
            Ending::Ran { .. } => {
                let event = EVENT_COMPLETED;
                tracing::info!(
                    lease_id,
                    wall_ms,
                    bundle_ms,
                    memory_ms,
                    sandbox_ms,
                    model_ms,
                    tools_ms,
                    report_ms,
                    first_token_ms,
                    event
                );
            }
            Ending::Failed { class, detail } => {
                // The spelling the report carries, so the log and the report
                // name a failure alike.
                let class = to_spelling(class);
                let class = class.as_deref();
                let event = EVENT_FAILED;
                tracing::info!(
                    lease_id,
                    class,
                    detail,
                    wall_ms,
                    bundle_ms,
                    memory_ms,
                    sandbox_ms,
                    model_ms,
                    tools_ms,
                    report_ms,
                    first_token_ms,
                    event
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "segments_tests.rs"]
mod tests;
