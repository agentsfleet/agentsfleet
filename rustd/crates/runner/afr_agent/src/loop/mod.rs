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
use guffbolt::Transcript;
use guffbolt::agents::{Agents, Children};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

pub use self::shared::RunLimits;
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
    limits: RunLimits,
}

impl Loop {
    /// A loop hosting `catalog`'s handlers and reaching models through
    /// `connect`, holding every run to the default [`RunLimits`].
    #[must_use]
    pub fn new(catalog: Catalog, connect: impl Connect + 'static) -> Self {
        Self {
            catalog,
            connect: Box::new(connect),
            limits: RunLimits::default(),
        }
    }

    /// The same loop, holding every run to `limits`.
    #[must_use]
    pub const fn with_limits(mut self, limits: RunLimits) -> Self {
        self.limits = limits;
        self
    }
}

#[async_trait::async_trait]
impl AgentEngine for Loop {
    async fn prepare_durable_task(
        &self,
        run: &af_wire::run::RunPayload<'_>,
        verbs: &dyn afr_tools::RunVerbs,
    ) -> Result<Option<crate::PreparedDurableTask>> {
        if run.durable_task.is_none() {
            return Ok(None);
        }
        crate::decision::prepare(run.fencing_token, verbs)
            .await
            .map(Some)
    }

    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<Needs> {
        self.connect.admit(policy)?;
        let sandbox = self.catalog.select(&policy.tools)?.needs_sandbox();
        Ok(Needs { sandbox })
    }

    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput> {
        let policy = &run.run.policy;
        let names = crate::offer::names(run.run);
        let selection = self.catalog.select(&names)?;
        let provider = self.connect.connect(run.run)?;
        let scrub = Scrub::new(policy)?;
        let span = spans::invoke_agent(policy);
        let mut prompt = Prompt::new(run.run, &run.memory);
        prompt.instructions = scrub.clean(prompt.instructions).into_inner();
        if let Some(prepared) = run.prepared_durable_task {
            let native = serde_json::json!({ "current_durable_task": prepared.inputs.current_durable_task,
                "accepted_interpretation": prepared.inputs.current_interpretation,
                "accepted_inputs": prepared.inputs.accepted_inputs });
            prompt.instructions.push_str("\n\nNative task scope: execute only the current task's accepted objective and items marked applies_here. Other accepted items retain their original reply deliveries but grant no permission to execute another task. A conversational answer does not complete the task. The coordinator assesses progress after complete action groups.\n");
            prompt
                .instructions
                .push_str(&scrub.clean(native.to_string()).into_inner());
        }
        // A fresh fleet chat opens on its thread's saved summary, and saves
        // the next one as its compactions cover earlier turns.
        let mut saved = if keeps_summary(&run) {
            let (model, pinned) = context::digests(
                &policy.context.model,
                policy.inference.as_ref(),
                policy.model_context_tokens,
                &prompt.fixed,
            )?;
            context::Saved::load(run.verbs, &run.run.run_id, model, pinned).await
        } else {
            None
        };
        let opening =
            construction::opening(&scrub, saved.as_mut(), &run.run.history, !names.is_empty())?;
        let recovery_boundary::RootSetup {
            opening,
            memory,
            state,
        } = recovery_boundary::prepare_root(&run, &scrub, opening, &prompt.message)?;
        // The run's state outlives its tools: the tools a run builds over its
        // memory, verbs, guard, sandbox and children borrow it, and every loop
        // borrows them. A recovered root brings its children back from its
        // saved state.
        let tools_run = shared::run_state(&run, memory, provider.as_ref(), self.limits);
        let caps = self.limits.caps;
        let (registry, requests) = state.as_ref().map_or_else(
            || Agents::new(caps),
            |recovered| Agents::restored(caps, &recovered.driver),
        );
        let backends = Backends::new(&tools_run, run.executor, self.catalog.transport())
            .with_agents(&registry);
        let backed = backends.tools(&names)?;
        let shared = Shared::new(
            &run,
            (&selection, backed),
            provider.as_ref(),
            &scrub,
            registry.clone(),
            prompt,
            &tools_run,
        );
        let driven = async {
            let mut children = Children::new(requests);
            let mut root = Harness::root(&shared, opening, saved);
            if let Some(recovered) = state {
                root.driver = recovered.driver;
                root.context = context::State::new(recovered.pinned);
                root.folded_through = recovered.folds.last().map_or(0, |fold| fold.sequence);
                root.folded_inputs = recovered.folds;
                root.assignment_answered = false;
                recovery_boundary::restored(&run);
                // A fresh opening already ends on the answer as the turn's
                // message; only a restored transcript still needs it.
                if let Some(answer) = run.run.event.question_answer() {
                    root.answered(&answer.message);
                }
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

/// Whether `run` keeps its thread's saved summary: a fresh fleet chat's
/// root. A recovered root resumes its own saved history, and a durable
/// task's run its task's.
fn keeps_summary(run: &AgentRun<'_>) -> bool {
    run.run.event.event_type == af_wire::turn::TurnKind::Chat
        && run.recovered_durable_task.is_none()
        && run.recovered_turn.is_none()
        && run.run.durable_task.is_none()
}

/// One loop in progress: the root, or a child.
pub(crate) struct Harness<'s, 'run> {
    task: Option<crate::PreparedDurableTask>,
    incorporated_input_sequence: u64,
    reconciled_effect_ordinal: u64,
    confirmed_effect_watermark: u64,
    durable_task_report: Option<af_wire::durable_task::DurableTaskReport>,
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
    /// Gives a person's answer to the question the run asked as the asking
    /// call's result. A run restored with no question waiting reads the
    /// answer as the person's next message instead.
    fn answered(&mut self, answer: &str) {
        let answer = self.shared.scrub.text(answer).into_owned();
        if self
            .driver
            .resume(&mut self.transcript, answer.clone())
            .is_none()
        {
            self.transcript.push(guffbolt::Message::User(answer));
        }
    }

    /// Runs the root's turns to their end, with a durable task's additions
    /// when it is one; `guffbolt` drives every child's.
    pub(crate) async fn drive(&mut self) -> Ending {
        let mut state = std::mem::take(&mut self.driver);
        let ending = if self.task.is_some() {
            guffbolt::drive(self, &mut durable_task::DurableTask, &mut state).await
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
mod durable_task;
mod durable_task_effects;
mod durable_task_evidence;
mod durable_task_replies;
mod durable_task_slice;
mod finish;
mod observe;
mod publication;
mod recovery_boundary;
pub(crate) mod shared;

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod durable_task_tests;

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
mod secret_tests;

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

#[cfg(test)]
mod quiet_tests;

#[cfg(test)]
mod saved_tests;

#[cfg(test)]
mod recovery_boundary_tests;

#[cfg(test)]
mod finished_tests;

#[cfg(test)]
mod answer_tests;
