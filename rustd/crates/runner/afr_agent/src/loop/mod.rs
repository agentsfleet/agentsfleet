//! The agent loop: turns until the model answers without a tool call, the
//! context cap is reached, the run is stopped, or the provider fails.
//!
//! One [`Harness`] is one loop: the run's root, or one turn of a child
//! `guffbolt` drives for a subagent call, which is the same loop over the
//! child's transcript. `guffbolt` makes each model call, runs and ends each
//! tool call, keeps the transcript, and owns the children: their tools,
//! narrowed from their parent's, their inboxes and their turns. The harness
//! is its conversation, fits each call's window (`context`), and hears its
//! work as its observer. What every loop of the run shares is [`Shared`]; the
//! ledger keeps what each call did. `guffbolt::agents` polls the children
//! beside the root loop, never spawned, so a run that ends takes every child
//! with it. Every tool a loop holds, the subagent ones and `propose_change`
//! included, sits in its router; the loop dispatches none by name.
//! `docs/architecture/runner_execution.md` §"Tool catalog" is the design.

use af_providers::Connect;
use af_secrets::Scrub;
use af_wire::policy::ExecutionPolicy;
use afr_tools::{Backends, Catalog, Selection};
use guffbolt::agents::{Agents, Children};
use guffbolt::{Hosted, Transcript};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

use self::shared::Shared;
use crate::context::Checkpoints;
use crate::engine::{AgentEngine, AgentRun, Needs, RunOutput};
use crate::error::Result;
use crate::events::Live;
use crate::prompt::Prompt;
use crate::router::Router;
use crate::spans;

const EVENT_CHECKPOINT_FAILED: &str = "memory_checkpoint_failed";
const EVENT_TURN_STARTED: &str = "provider_turn_started";
const EVENT_TURN_COMPLETED: &str = "provider_turn_completed";
const EVENT_PROVIDER_FAILED: &str = "provider_turn_failed";
/// Why a turn the run stopped did not complete.
const REASON_STOPPED: &str = "run_stopped";
/// One of a child's turns starts.
const EVENT_CHILD_STARTED: &str = "child_started";
/// One of a child's turns ended.
const EVENT_CHILD_ENDED: &str = "child_ended";

/// How a loop's turns ended, a failure being the provider's.
pub(crate) type Ending = guffbolt::Ending<af_providers::Error>;

/// The agent engine that runs the model against the run's tools.
#[derive(Debug)]
pub struct Loop {
    catalog: Catalog,
    connect: Box<dyn Connect>,
}

impl Loop {
    /// A loop hosting `catalog`'s handlers and reaching models through
    /// `connect`.
    #[must_use]
    pub fn new(catalog: Catalog, connect: impl Connect + 'static) -> Self {
        Self {
            catalog,
            connect: Box::new(connect),
        }
    }
}

#[async_trait::async_trait]
impl AgentEngine for Loop {
    async fn prepare_task(
        &self,
        run: &af_wire::run::RunPayload<'_>,
        verbs: &dyn afr_tools::RunVerbs,
        meter: &crate::Meter,
        stop: &CancellationToken,
    ) -> Result<Option<crate::PreparedTask>> {
        if !run.durable_tasks {
            return Ok(None);
        }
        let provider = self.connect.connect(run)?;
        crate::decision::prepare(run, verbs, provider.as_ref(), meter, stop)
            .await
            .map(Some)
    }

    /// A setup run reaches its model the way a fleet run does, and holds no
    /// catalog tool: its one tool is built per run (`crate::setup`).
    async fn setup(&self, run: crate::setup::SetupRun<'_>) -> Result<crate::setup::SetupOutput> {
        crate::setup::run(self.connect.as_ref(), run).await
    }

    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<Needs> {
        self.connect.admit(policy)?;
        let sandbox = self.catalog.select(&policy.tools)?.needs_sandbox();
        Ok(Needs { sandbox })
    }

    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput> {
        let policy = &run.run.policy;
        if run.run.durable_tasks && run.prepared_task.is_none() {
            return Err(crate::decision::unavailable());
        }
        let tools = if run
            .prepared_task
            .is_some_and(|prepared| !prepared.allow_actions)
        {
            &[][..]
        } else {
            &policy.tools
        };
        let selection = self.catalog.select(tools)?;
        let provider = self.connect.connect(run.run)?;
        let scrub = Scrub::new(policy)?;
        let span = spans::invoke_agent(policy);
        let prompt = Prompt::new(run.run);
        let mut instructions = scrub.clean(prompt.instructions).into_inner();
        if let Some(prepared) = run.prepared_task {
            let native = serde_json::json!({ "current_task": prepared.inputs.current_task,
                "accepted_interpretation": prepared.inputs.current_interpretation,
                "accepted_inputs": prepared.inputs.accepted_inputs });
            instructions.push_str("\n\nNative task scope: execute only the current task's accepted objective and items marked applies_here. Other accepted items retain their original reply deliveries but grant no permission to execute another task. A conversational answer does not complete the task. The coordinator assesses progress after complete action groups.\n");
            instructions.push_str(&scrub.clean(native.to_string()).into_inner());
        }
        // Every earlier turn passes the scrub the current message does: a
        // secret said in an earlier message is still a secret.
        let opening = construction::opening(&scrub, prompt.history);
        let recovery_boundary::RootSetup {
            opening,
            memory,
            state,
        } = recovery_boundary::prepare_root(&run, &scrub, opening, &prompt.message)?;
        // The run's state outlives its tools: the tools a run builds over its
        // memory, verbs, guard, sandbox and children borrow it, and every loop
        // borrows them. A recovered root brings its children back from its
        // saved state.
        let tools_run = shared::run_state(&run, memory, provider.as_ref());
        let (registry, requests) = state.as_ref().map_or_else(
            || Agents::new(shared::CAPS),
            |recovered| Agents::restored(shared::CAPS, &recovered.driver),
        );
        let backends = Backends::new(&tools_run, run.executor, self.catalog.transport())
            .with_agents(&registry);
        let backed = backends.tools(tools)?;
        let shared = Shared::new(
            &run,
            (&selection, backed),
            provider.as_ref(),
            &scrub,
            registry.clone(),
            instructions,
            &tools_run,
        );
        let driven = async {
            let mut children = Children::new(requests);
            let mut root = Harness::root(&shared, opening);
            if let Some(recovered) = state {
                root.driver = recovered.driver;
                root.context = context::State::new(recovered.pinned);
                root.folded_through = recovered.folds.last().map_or(0, |fold| fold.sequence);
                root.folded_inputs = recovered.folds;
                root.assignment_answered = false;
                recovery_boundary::restored(&run);
            }
            let ending = children.beside(&shared, root.drive()).await;
            // Every child is dropped before the ledger closes, so each one's
            // open call ends `interrupted` in the trace the report carries.
            let pause = children.end_all(&shared.registry);
            root.finish(ending, pause.as_ref()).await
        };
        Ok(driven.instrument(span).await)
    }
}

/// One loop in progress: the root, or a child.
pub(crate) struct Harness<'s, 'run> {
    task: Option<crate::PreparedTask>,
    incorporated_input_sequence: u64,
    reconciled_effect_ordinal: u64,
    confirmed_effect_watermark: u64,
    task_report: Option<af_wire::task::TaskReport>,
    pub(crate) shared: &'s Shared<'run>,
    /// The child this loop is a turn of: zero for the root.
    pub(crate) id: u64,
    /// How many loops this one is inside: zero for the root.
    pub(crate) depth: u8,
    /// Cancelled when this loop is to end: the run's token for the root, a
    /// descendant of its parent's for a child.
    pub(crate) stop: CancellationToken,
    /// The tools the run was offered; `guffbolt` narrows a child's.
    pub(crate) selection: &'s Selection<'run>,
    /// Every tool this loop holds, and how a call reaches one.
    router: Router<'s, 'run>,
    hosted: Vec<Hosted>,
    live: Live<'s>,
    checkpoints: Checkpoints,
    context: context::State,
    /// The conversation `guffbolt` keeps for this loop.
    transcript: Transcript,
    driver: guffbolt::DriveState,
    /// The last folded message this loop took, by sequence; 0 before the
    /// first. A child takes none.
    folded_through: i64,
    folded_inputs: Vec<crate::engine::FoldRef>,
    turn_checkpoint: Option<af_wire::run::TurnResume>,
    /// Whether the daemon's assignment already answered this loop's next fold ask:
    /// its run said no later message waited, so the ask before the first
    /// model call would be assigned nothing. Spent by that ask.
    assignment_answered: bool,
}

impl Harness<'_, '_> {
    /// Runs the root's turns to their end, with a durable task's additions
    /// when it is one; `guffbolt` drives every child's.
    pub(crate) async fn drive(&mut self) -> Ending {
        let mut state = std::mem::take(&mut self.driver);
        let ending = if self.task.is_some() {
            guffbolt::drive(self, &mut task::Task, &mut state).await
        } else {
            guffbolt::drive(self, &mut recovery_boundary::Root, &mut state).await
        };
        self.driver = state;
        ending
    }
}

mod construction;
mod context;
mod conversation;
mod finish;
mod observe;
mod publication;
mod recovery_boundary;
pub(crate) mod shared;
mod task;
mod task_effects;
mod task_evidence;
mod task_replies;
mod task_slice;

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod task_tests;

#[cfg(test)]
mod image_tests;

#[cfg(test)]
mod budget_tests;

#[cfg(test)]
mod history_tests;

#[cfg(test)]
mod provider_failure_tests;

#[cfg(test)]
mod turn_tests;

#[cfg(test)]
mod end_tests;

#[cfg(test)]
mod memory_round_trip_tests;
#[cfg(test)]
mod memory_tests;

#[cfg(test)]
mod session_tests;

#[cfg(test)]
mod checkout_tests;

#[cfg(test)]
mod compaction_tests;

#[cfg(test)]
mod fold_tests;
