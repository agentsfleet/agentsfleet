//! The seam between the supervisor and whatever runs a turn.

mod boundary;
pub use boundary::{
    CommittedRecovery, FileRecoveryPermit, RecoveryBoundarySink, RecoveryReceipt, RecoverySave,
    RecoverySkip, RecoverySnapshot, RecoveryState,
};
mod meter;
pub(crate) use boundary::RecoveryWitness;
pub use meter::Meter;
mod recovery;
mod root_recovery;
pub(crate) use af_wire::run::FoldRef;
pub(crate) use root_recovery::RootRecovery;
mod turn_recovery;
pub use turn_recovery::{TurnRecovery, TurnSnapshot};
#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod turn_recovery_tests;
pub use recovery::TaskRecovery;

use std::fmt;

use crate::ExecutionResult;
use af_egress::Mint;
use af_wire::activity::ActivityFrame;
use af_wire::memory::MemoryDelta;
use af_wire::policy::ExecutionPolicy;
use af_wire::run::RunPayload;
use af_wire::tool_detail::ToolCallRecord;
use af_wire::tool_trace::ToolTrace;
use afr_executor::ExecutorSource;
use afr_memory::Seed;
use afr_tools::RunVerbs;
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

/// One message a person sent the fleet while the run works, assigned to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folded {
    /// Stable turn identity retained for exact checkpoint publication and adoption.
    pub turn_id: String,
    /// Its place in the fleet's order; the report names the last one read.
    pub sequence: i64,
    /// What the person wrote, unscrubbed: the run masks it with everything else.
    pub text: String,
}

/// Where a run's root takes the messages a person sends while it works.
///
/// The loop asks for an assignment before each model call. An assignment never
/// fails the run: an implementation logs what went wrong and answers nothing,
/// and the next assignment asks again from the same place, so a lost answer
/// loses no message.
#[async_trait::async_trait]
pub trait Inbox: Send + Sync + fmt::Debug {
    /// The messages sent after `after_sequence`, oldest first.
    async fn assign(&self, after_sequence: i64) -> Vec<Folded>;
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
    async fn push(&self, memory: Vec<MemoryDelta<'static>>) -> Result<()>;
}

/// Everything one run is given.
pub struct AgentRun<'run> {
    /// Optional live capture owner; absence cannot acknowledge a checkpoint.
    pub recovery: Option<&'run dyn RecoveryBoundarySink>,
    /// Authenticated history restored with the same frozen file generation.
    pub recovered_task: Option<&'run TaskRecovery>,
    /// Exact ordinary generation restored and adopted before the agent starts.
    pub recovered_turn: Option<&'run TurnRecovery>,
    /// Native intent and recovery read completed before sandbox construction.
    pub prepared_task: Option<&'run crate::PreparedTask>,
    /// The run being run.
    pub run: &'run RunPayload<'run>,
    /// The fleet's memory, loaded before the run started, and where a
    /// recall past it asks.
    pub memory: Seed<'run>,
    /// Where the run's sandbox-side calls get the sandbox's executor. The
    /// run's build fills it while the run goes on; a run with no sandbox
    /// passes one that answers `NoSandbox`.
    pub executor: &'run dyn ExecutorSource,
    /// Mints the credentials the run's policy names, under the held run.
    pub mint: &'run dyn Mint,
    /// The `agentsfleetd` verbs the schedule and message tools reach, fenced
    /// by the held run.
    pub verbs: &'run dyn RunVerbs,
    /// Writes the run's memory back every `memory_checkpoint_every` calls.
    pub checkpoint: &'run dyn Checkpoint,
    /// Where the root takes the messages a person sends while it works.
    pub inbox: &'run dyn Inbox,
    /// Where activity frames go.
    pub events: &'run dyn EventSink,
    /// The tokens the run has spent so far, which the engine adds to after
    /// each turn and the supervisor reads into every renewal.
    pub meter: &'run Meter,
    /// Cancelled when the run ends early. The engine closes every open call
    /// `interrupted` and returns what it has.
    pub stop: &'run CancellationToken,
}

impl fmt::Debug for AgentRun<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentRun")
            .field("run_id", &self.run.run_id)
            .field("memory", &self.memory.len())
            .field("executor", &self.executor)
            .finish_non_exhaustive()
    }
}

/// What a finished run hands back to the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    /// Private history paired with frozen files by the supervisor's recovery owner.
    pub recovery: Option<TaskRecovery>,
    /// Native decision-backed disposition at the last completed safe boundary.
    /// A conversational answer alone never supplies a durable task outcome.
    pub task: Option<af_wire::task::TaskReport>,
    /// The result the report carries.
    pub result: ExecutionResult<'static>,
    /// The memory to push, fenced, before the report.
    pub memory: Vec<MemoryDelta<'static>>,
    /// Known calls and their outcomes, including an empty trace when no tool ran.
    /// None means trace data was unavailable.
    pub trace: Option<ToolTrace<'static>>,
    /// Each finished call's full record, posted before the report.
    pub records: Vec<ToolCallRecord<'static>>,
    /// The last folded message a completed model call read, when the run
    /// answered: each message assigned up to it is answered by this result.
    pub folded_through_sequence: Option<i64>,
}

/// What a run needs prepared before its turn runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    /// Whether any of its tools runs inside a sandbox. A run whose tools all
    /// run in the supervisor starts none.
    pub sandbox: bool,
}

/// Runs one run's turn.
#[async_trait::async_trait]
pub trait AgentEngine: Send + Sync + fmt::Debug {
    /// Establishes native intent before provisioning any sandbox or checkout.
    /// Engines without durable coordination refuse durable runs explicitly.
    async fn prepare_task(
        &self,
        run: &RunPayload<'_>,
        _verbs: &dyn RunVerbs,
        _meter: &Meter,
        _stop: &CancellationToken,
    ) -> Result<Option<crate::PreparedTask>> {
        if run.durable_tasks {
            return Err(crate::decision::unavailable());
        }
        Ok(None)
    }
    /// Says what a run under `policy` needs, before anything is prepared
    /// for it.
    ///
    /// # Errors
    /// The policy names a tool this engine cannot host. The run is refused
    /// rather than run with a quieter tool set than its author wrote.
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<Needs>;

    /// Runs the turn to its end.
    ///
    /// A failure the fleet caused is a result, not an error: it comes back as
    /// a failed [`ExecutionResult`]. An error here means the engine itself
    /// could not run.
    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput>;
}

#[cfg(test)]
mod tests;
