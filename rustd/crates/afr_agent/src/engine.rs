//! The seam between the supervisor and whatever runs a turn.

use std::fmt;

use afd_wire::activity::ActivityFrame;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::MemoryDelta;
use afd_wire::report::ExecutionResult;
use afr_executor::Executor;

use crate::error::Result;

/// Where a run's activity frames go.
///
/// Emitting never blocks the run: the supervisor's sender drops and counts
/// what the daemon cannot take. Any `Fn(ActivityFrame)` is a sink, so a test
/// passes a closure where production passes the activity sender.
pub trait EventSink: Send + Sync {
    /// Hands one frame to the live tail.
    fn emit(&self, frame: ActivityFrame<'static>);
}

impl<F> EventSink for F
where
    F: Fn(ActivityFrame<'static>) + Send + Sync,
{
    fn emit(&self, frame: ActivityFrame<'static>) {
        self(frame);
    }
}

/// Everything one run is given.
pub struct AgentRun<'run> {
    /// The lease being run.
    pub lease: &'run LeasePayload<'run>,
    /// The fleet's memory, hydrated before the run started.
    pub memory: &'run [MemoryDelta<'run>],
    /// The sandbox's executor, when the lease's tools need one.
    pub executor: Option<&'run dyn Executor>,
    /// Where activity frames go.
    pub events: &'run dyn EventSink,
}

impl fmt::Debug for AgentRun<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentRun")
            .field("lease_id", &self.lease.lease_id)
            .field("memory", &self.memory.len())
            .field("executor", &self.executor)
            .finish_non_exhaustive()
    }
}

/// What a finished run hands back to the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    /// The result the report carries.
    pub result: ExecutionResult<'static>,
    /// The memory to push, fenced, before the report.
    pub memory: Vec<MemoryDelta<'static>>,
}

/// Runs one lease's turn.
#[async_trait::async_trait]
pub trait AgentEngine: Send + Sync + fmt::Debug {
    /// Runs the turn to its end.
    ///
    /// A failure the fleet caused is a result, not an error: it comes back as
    /// a failed [`ExecutionResult`]. An error here means the engine itself
    /// could not run.
    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput>;
}
