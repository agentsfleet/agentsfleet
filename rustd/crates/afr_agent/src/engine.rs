//! The seam between the supervisor and whatever runs a turn.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use afd_wire::activity::ActivityFrame;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::MemoryDelta;
use afd_wire::policy::ExecutionPolicy;
use afd_wire::report::ExecutionResult;
use afd_wire::tool_detail::ToolCallRecord;
use afd_wire::tool_trace::ToolTrace;
use afr_egress::Mint;
use afr_executor::Executor;
use afr_memory::Seed;
use afr_providers::Usage;
use afr_tools::LeaseVerbs;
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

/// Writes a run's memory back while it runs, fenced like the push before the
/// report.
///
/// Best effort, as that push is: the final push carries every entry again and
/// is the one the report waits for, so a checkpoint that fails is logged by
/// its implementation and the run goes on.
#[async_trait::async_trait]
pub trait Checkpoint: Send + Sync + fmt::Debug {
    /// Writes `memory` back.
    ///
    /// # Errors
    /// [`Error::checkpoint`](crate::Error::checkpoint): the write failed. The
    /// run goes on; the push before the report carries every entry again.
    async fn push(&self, memory: Vec<MemoryDelta<'static>>) -> crate::Result<()>;
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
    /// Mints the credentials the lease's policy names, under the held lease.
    pub mint: &'run dyn Mint,
    /// The `agentsfleetd` verbs the schedule and message tools reach, fenced
    /// by the held lease.
    pub verbs: &'run dyn LeaseVerbs,
    /// Writes the run's memory back every `memory_checkpoint_every` calls.
    pub checkpoint: &'run dyn Checkpoint,
    /// Where activity frames go.
    pub events: &'run dyn EventSink,
    /// The tokens the run has spent so far, which the engine adds to after
    /// each turn and the supervisor reads into every renewal.
    pub meter: &'run Meter,
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

/// The tokens a run has spent so far, readable while it runs.
///
/// The engine adds each turn's usage, and the supervisor reports the running
/// total with every lease renewal, so the daemon meters a run before it ends.
/// Each count is its own atomic: a read between two adds may see one turn's
/// input without its output, and the next renewal carries both.
#[derive(Debug, Default)]
pub struct Meter {
    input: AtomicU64,
    cached_input: AtomicU64,
    output: AtomicU64,
}

impl Meter {
    /// Adds one turn's tokens.
    pub fn add(&self, usage: Usage) {
        self.input.fetch_add(usage.input, Ordering::Relaxed);
        self.cached_input
            .fetch_add(usage.cached_input, Ordering::Relaxed);
        self.output.fetch_add(usage.output, Ordering::Relaxed);
    }

    /// Everything the run has spent so far.
    #[must_use]
    pub fn read(&self) -> Usage {
        Usage {
            input: self.input.load(Ordering::Relaxed),
            cached_input: self.cached_input.load(Ordering::Relaxed),
            output: self.output.load(Ordering::Relaxed),
        }
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
