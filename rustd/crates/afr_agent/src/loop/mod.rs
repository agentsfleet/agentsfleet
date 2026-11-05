//! The agent loop: turns until the model answers without a tool call, the
//! context cap is reached, the lease is stopped, or the provider fails.
//!
//! One [`Harness`] drives one loop: the run's root, or a child a nested tool
//! started, which is the same loop over a task of the model's with a narrower
//! selection. What every loop of the run shares is [`Shared`]; the ledger
//! keeps what each call did. Children are polled beside the root loop, never
//! spawned, so a run that ends takes every child with it.
//! `docs/architecture/runner_execution.md` §"Tool catalog" is the design.

use afd_wire::policy::ExecutionPolicy;
use afr_providers::{Call, Connect, Hosted, Message, Replay, ToolSpec};
use afr_secrets::Scrub;
use afr_tools::{Catalog, Selection};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

use self::children::Children;
use self::shared::Shared;
use crate::context::Checkpoints;
use crate::engine::{AgentEngine, AgentRun, Needs, RunOutput};
use crate::error::Result;
use crate::events::Live;
use crate::nested::{self, Guard, Registry, Seat, Tether};
use crate::offer;
use crate::prompt::Prompt;
use crate::router::{self, Router};
use crate::spans;

const EVENT_CHECKPOINT_FAILED: &str = "memory_checkpoint_failed";
const EVENT_TURN_STARTED: &str = "provider_turn_started";
const EVENT_TURN_COMPLETED: &str = "provider_turn_completed";
const EVENT_PROVIDER_FAILED: &str = "provider_turn_failed";
/// Why a turn the lease stopped did not complete.
const REASON_STOPPED: &str = "lease_stopped";
/// What joins two messages a parent sent a child before one turn.
const INPUT_JOIN: &str = "\n\n";

/// The agent engine that runs the model against the lease's tools.
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
        lease: &afd_wire::lease::LeasePayload<'_>,
        verbs: &dyn afr_tools::LeaseVerbs,
        meter: &crate::Meter,
        stop: &CancellationToken,
    ) -> Result<Option<crate::PreparedTask>> {
        if !lease.durable_tasks {
            return Ok(None);
        }
        let provider = self.connect.connect(lease)?;
        crate::decision::prepare(lease, verbs, provider.as_ref(), meter, stop)
            .await
            .map(Some)
    }

    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<Needs> {
        self.connect.admit(policy)?;
        let sandbox = self.catalog.select(&policy.tools)?.needs_sandbox();
        Ok(Needs { sandbox })
    }

    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput> {
        let policy = &run.lease.policy;
        if run.lease.durable_tasks && run.prepared_task.is_none() {
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
        let provider = self.connect.connect(run.lease)?;
        let scrub = Scrub::new(policy)?;
        let span = spans::invoke_agent(policy);
        let prompt = Prompt::new(run.lease);
        let mut instructions = scrub.clean(prompt.instructions).into_inner();
        if let Some(prepared) = run.prepared_task {
            let native = serde_json::json!({ "current_task": prepared.inputs.current_task,
                "accepted_interpretation": prepared.inputs.current_interpretation,
                "accepted_inputs": prepared.inputs.accepted_inputs });
            instructions.push_str("\n\nNative task scope: execute only the current task's accepted objective and items marked applies_here. Other accepted items retain their original reply obligations but grant no permission to execute another task. A conversational answer does not complete the task. The coordinator assesses progress after complete action groups.\n");
            instructions.push_str(&scrub.clean(native.to_string()).into_inner());
        }
        // Every earlier turn passes the scrub the current message does: a
        // secret said in an earlier message is still a secret.
        let mut opening: Vec<Message> = prompt
            .history
            .into_iter()
            .flat_map(|(asked, answered)| {
                [
                    Message::User(scrub.clean(asked).into_inner()),
                    Message::Assistant {
                        text: scrub.clean(answered).into_inner(),
                        calls: Vec::new(),
                        replay: Replay::default(),
                    },
                ]
            })
            .collect();
        if let Some(recovered) = run.recovered_task {
            let current = run
                .prepared_task
                .and_then(|prepared| prepared.inputs.current_task.as_ref())
                .ok_or_else(crate::decision::unavailable)?;
            opening = crate::TaskRecovery::restore(
                &recovered.history,
                current,
                &policy.context.model,
                &policy.provider,
                &scrub,
            )?;
            opening.push(Message::User("Resume the accepted objective from this authenticated history. Reconcile native inputs and confirmed effects before taking another action.".to_owned()));
        } else {
            opening.push(Message::User(scrub.clean(prompt.message).into_inner()));
        }
        let (registry, requests) = Registry::new();
        let shared = Shared::new(
            &run,
            &selection,
            provider.as_ref(),
            &scrub,
            registry,
            instructions,
        );
        let driven = async {
            let mut children = Children::new(requests);
            let mut root = Harness::root(&shared, opening);
            let ending = children.beside(&shared, root.drive()).await;
            // Every child is dropped before the ledger closes, so each one's
            // open call ends `interrupted` in the trace the report carries.
            children.end_all();
            root.finish(ending).await
        };
        Ok(driven.instrument(span).await)
    }
}

/// How a loop's turns ended.
pub(crate) enum Ending {
    Answered(String),
    Failed(afr_providers::Error),
    Stopped,
}

/// One loop in progress: the root, or a child.
pub(crate) struct Harness<'s, 'run> {
    task: Option<crate::PreparedTask>,
    incorporated_input_sequence: u64,
    reconciled_effect_ordinal: u64,
    confirmed_effect_watermark: u64,
    task_report: Option<afd_wire::task::TaskReport>,
    group_start: usize,
    pub(crate) shared: &'s Shared<'run>,
    /// How many loops this one is inside: zero for the root.
    pub(crate) depth: u8,
    /// Cancelled when this loop is to end: the run's token for the root, a
    /// descendant of its parent's for a child.
    pub(crate) stop: CancellationToken,
    /// The tools this loop is offered; a child's selection narrows it.
    pub(crate) selection: &'s Selection<'run>,
    router: Router<'s>,
    specs: Vec<ToolSpec<'s>>,
    hosted: Vec<Hosted>,
    live: Live<'s>,
    checkpoints: Checkpoints,
    context: context::State,
    messages: Vec<Message>,
    /// The child this loop is, when it is one: its guard, and what its
    /// parent sent, read into its next turn.
    pub(crate) child: Option<Tether<'s, 'run>>,
}

impl<'s, 'run> Harness<'s, 'run> {
    /// The run's root loop, over every tool the lease was offered, opening
    /// with the fleet's earlier turns and then the event's message.
    fn root(shared: &'s Shared<'run>, opening: Vec<Message>) -> Self {
        let selection = shared.selection;
        let mut root = Self {
            task: shared.prepared_task.cloned(),
            incorporated_input_sequence: shared
                .recovered_task
                .map_or(0, |recovery| recovery.task.covered_input_sequence),
            reconciled_effect_ordinal: shared
                .recovered_task
                .map_or(0, |recovery| recovery.task.confirmed_effect_watermark),
            confirmed_effect_watermark: shared
                .recovered_task
                .map_or(0, |recovery| recovery.task.confirmed_effect_watermark),
            task_report: None,
            group_start: if shared.recovered_task.is_some() {
                0
            } else {
                opening.len()
            },
            shared,
            depth: 0,
            stop: shared.stop.clone(),
            selection,
            router: Router::new(selection, shared.executor),
            specs: offer::specs(selection),
            hosted: offer::hosted(selection),
            live: Live::new(shared.events, shared.scrub, shared.started),
            checkpoints: Checkpoints::new(shared.context),
            context: context::State::new(if shared.recovered_task.is_some() {
                0
            } else {
                opening.len().saturating_sub(1)
            }),
            messages: opening,
            child: None,
        };
        if root.task.is_some() {
            root.live.hide_reasoning();
        }
        if root.publication_allowed() {
            root.specs.push(publication::spec());
        }
        root
    }

    /// A child loop over `selection`, opening with its seat's task. Its text
    /// goes to no frame: its parent reads it through the call that started
    /// it. It writes no checkpoint: the push before the report carries what
    /// it stored.
    pub(crate) fn child(
        shared: &'s Shared<'run>,
        selection: &'s Selection<'run>,
        seat: Seat,
        guard: &'s Guard<'s, 'run>,
    ) -> Self {
        Self {
            task: None,
            incorporated_input_sequence: 0,
            reconciled_effect_ordinal: 0,
            confirmed_effect_watermark: 0,
            task_report: None,
            group_start: 1,
            shared,
            depth: seat.depth,
            stop: seat.stop,
            selection,
            router: Router::new(selection, shared.executor),
            specs: offer::specs(selection),
            hosted: offer::hosted(selection),
            live: Live::silent(shared.scrub, shared.started),
            checkpoints: Checkpoints::never(),
            context: context::State::new(0),
            messages: vec![Message::User(shared.scrub.clean(seat.task).into_inner())],
            child: Some(Tether {
                guard,
                input: seat.input,
            }),
        }
    }

    /// Runs the loop's turns to their end.
    pub(crate) async fn drive(&mut self) -> Ending {
        let mut turns: u64 = 0;
        loop {
            turns += 1;
            self.read_input();
            if self.incorporate_task_inputs().await.is_err() {
                break Ending::Stopped;
            }
            if self.reconcile_task_effects().await.is_err() {
                break Ending::Stopped;
            }
            if self.answer_task_inputs().await.is_err() {
                break Ending::Stopped;
            }
            if let Some(answer) = self.accepted_answer() {
                break Ending::Answered(answer);
            }
            if turns == 1 && self.shared.recovered_task.is_some() {
                if let Some(ending) = self
                    .assess_task("Recovery reconciled; no new action has run.")
                    .await
                {
                    break ending;
                }
            }
            if let Err(error) = self.preflight().await {
                if self.stop.is_cancelled() {
                    break Ending::Stopped;
                }
                break Ending::Failed(afr_providers::Error::lost(error));
            }
            self.group_start = self.messages.len();
            let turn = match self.turn(turns, false).await {
                Some(Ok(turn)) => turn,
                Some(Err(failure)) => break Ending::Failed(failure),
                None => break Ending::Stopped,
            };
            if self.task.is_some() && (!turn.finished || turn.cut) {
                break Ending::Stopped;
            }
            if turn.calls.is_empty() {
                // An empty first turn must not turn a silent failure into task success.
                if turns == 1 && turn.text.trim().is_empty() {
                    break Ending::Failed(afr_providers::Error::silent(turn.cut));
                }
                if let Some(ending) = self.assess_task(&turn.text).await {
                    break ending;
                }
                if self.task_report.as_ref().is_some_and(|report| {
                    report.disposition == afd_wire::task::TaskDisposition::Continue
                }) {
                    let said = self.remembered(turn.text, turn.calls, turn.replay);
                    self.messages.push(said);
                    self.messages.push(Message::User("The accepted task goal remains incomplete. Continue the next bounded action group using the current evidence.".to_owned()));
                    continue;
                }
                break Ending::Answered(turn.text);
            }
            let mut results = Vec::with_capacity(turn.calls.len());
            for call in &turn.calls {
                match self.call(call, turn.cut).await {
                    Some(result) => results.push(result),
                    None => break,
                }
                if self.checkpoints.due() {
                    self.checkpoint().await;
                }
            }
            let said = self.remembered(turn.text, turn.calls, turn.replay);
            self.messages.push(said);
            self.messages.extend(results);
            if self.stop.is_cancelled() {
                break Ending::Stopped;
            }
            if self.incorporate_task_inputs().await.is_err()
                || self.reconcile_task_effects().await.is_err()
            {
                break Ending::Stopped;
            }
            if let Some(ending) = self.assess_task("").await {
                break ending;
            }
        }
    }

    /// Runs one call to its end, or answers it unrun when its turn was `cut`
    /// at the output limit; `None` when the loop was stopped. A nested tool
    /// the loop was offered runs here, beside the turn; every other call
    /// goes to the router.
    async fn call(&self, call: &Call, cut: bool) -> Option<Message> {
        let handler = async {
            if cut {
                return router::cut(&call.name);
            }
            if call.name == publication::NAME {
                return self.publish_task(&call.arguments).await;
            }
            if let Some(output) = nested::run(self, call).await {
                return output;
            }
            let lease = &self.shared.lease;
            self.router
                .dispatch(&call.name, &call.arguments, lease)
                .await
        };
        let (text, image) = tokio::select! {
            biased;
            () = self.stop.cancelled() => return None,
            answered = self.shared.ledger.call(call, handler) => answered,
        };
        if let Some(child) = &self.child {
            child.guard.called();
        }
        // The image a call read rides its result alone; the ledger, the trace
        // and the frames saw the text.
        let image = image.map(attach::image_input);
        Some(Message::ToolResult {
            call_id: call.id.clone(),
            output: text.into_inner(),
            image,
        })
    }
}

mod attach;
mod children;
mod context;
mod conversation;
mod finish;
mod model_turn;
mod publication;
pub(crate) mod shared;
mod task;
mod task_effects;
mod task_evidence;
mod task_replies;

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
mod memory_tests;

#[cfg(test)]
mod session_tests;

#[cfg(test)]
mod checkout_tests;

#[cfg(test)]
mod compaction_tests;
