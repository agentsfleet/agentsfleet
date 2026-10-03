//! The seam between the supervisor and whatever runs a turn.

use std::fmt;

use afd_wire::activity::ActivityFrame;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::MemoryDelta;
use afd_wire::policy::ExecutionPolicy;
use afd_wire::report::ExecutionResult;
use afd_wire::tool_detail::ToolCallRecord;
use afd_wire::tool_trace::ToolTrace;
use afr_executor::Executor;
use afr_memory::Seed;
use tokio_util::sync::CancellationToken;

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
    /// The fleet's memory, hydrated before the run started, and where a
    /// recall past it asks.
    pub memory: Seed<'run>,
    /// The sandbox's executor, when the lease's tools need one.
    pub executor: Option<&'run dyn Executor>,
    /// Where activity frames go.
    pub events: &'run dyn EventSink,
    /// Cancelled when the lease ends early. The engine closes every open call
    /// `interrupted` and returns what it has.
    pub stop: &'run CancellationToken,
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
    /// Every call the run made and how it ended, for the report; none for a
    /// run that called no tool.
    pub trace: Option<ToolTrace<'static>>,
    /// Each finished call's full record, posted before the report.
    pub records: Vec<ToolCallRecord<'static>>,
}

/// What a lease needs prepared before its turn runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    /// Whether any of its tools runs inside a sandbox. A lease whose tools all
    /// run in the supervisor starts none.
    pub sandbox: bool,
}

/// Runs one lease's turn.
#[async_trait::async_trait]
pub trait AgentEngine: Send + Sync + fmt::Debug {
    /// Says what a lease under `policy` needs, before anything is prepared
    /// for it.
    ///
    /// # Errors
    /// The policy names a tool this engine cannot host. The lease is refused
    /// rather than run with a quieter tool set than its author wrote.
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<Needs>;

    /// Runs the turn to its end.
    ///
    /// A failure the fleet caused is a result, not an error: it comes back as
    /// a failed [`ExecutionResult`]. An error here means the engine itself
    /// could not run.
    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput>;
}
