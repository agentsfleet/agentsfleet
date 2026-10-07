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
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

use self::children::Children;
use self::shared::Shared;
use crate::context::{Budget, Checkpoints};
use crate::engine::{AgentEngine, AgentRun, Needs, RunOutput};
use crate::error::Result;
use crate::events::Live;
use crate::nested::{self, Guard, Registry, Seat};
use crate::offer;
use crate::prompt::Prompt;
use crate::router::{self, Router};
use crate::spans;

const EVENT_CAP_REACHED: &str = "context_cap_reached";
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
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<Needs> {
        self.connect.admit(policy)?;
        let sandbox = self.catalog.select(&policy.tools)?.needs_sandbox();
        Ok(Needs { sandbox })
    }

    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput> {
        let policy = &run.lease.policy;
        let selection = self.catalog.select(&policy.tools)?;
        let provider = self.connect.connect(run.lease)?;
        let scrub = Scrub::new(policy)?;
        let span = spans::invoke_agent(policy);
        let prompt = Prompt::new(run.lease);
        let instructions = scrub.clean(prompt.instructions).into_inner();
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
        opening.push(Message::User(scrub.clean(prompt.message).into_inner()));
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
    budget: Budget,
    messages: Vec<Message>,
    /// What a parent sent this child, read into its next turn.
    input: Option<mpsc::UnboundedReceiver<String>>,
    /// The child this loop is, when it is one.
    pub(crate) child: Option<Guard<'s, 'run>>,
}

impl<'s, 'run> Harness<'s, 'run> {
    /// The run's root loop, over every tool the lease was offered, opening
    /// with the fleet's earlier turns and then the event's message.
    fn root(shared: &'s Shared<'run>, opening: Vec<Message>) -> Self {
        let selection = shared.selection;
        Self {
            shared,
            depth: 0,
            stop: shared.stop.clone(),
            selection,
            router: Router::new(selection, shared.executor),
            specs: offer::specs(selection),
            hosted: offer::hosted(selection),
            live: Live::new(shared.events, shared.scrub, shared.started),
            checkpoints: Checkpoints::new(shared.context),
            budget: Budget::new(shared.context),
            messages: opening,
            input: None,
            child: None,
        }
    }

    /// A child loop over `selection`, opening with its seat's task. Its text
    /// goes to no frame: its parent reads it through the call that started
    /// it. It writes no checkpoint: the push before the report carries what
    /// it stored.
    pub(crate) fn child(
        shared: &'s Shared<'run>,
        selection: &'s Selection<'run>,
        seat: Seat,
        guard: Guard<'s, 'run>,
    ) -> Self {
        Self {
            shared,
            depth: seat.depth,
            stop: seat.stop,
            selection,
            router: Router::new(selection, shared.executor),
            specs: offer::specs(selection),
            hosted: offer::hosted(selection),
            live: Live::silent(shared.scrub, shared.started),
            checkpoints: Checkpoints::never(),
            budget: Budget::new(shared.context),
            messages: vec![Message::User(shared.scrub.clean(seat.task).into_inner())],
            input: Some(seat.input),
            child: Some(guard),
        }
    }

    /// Runs the loop's turns to their end.
    pub(crate) async fn drive(&mut self) -> Ending {
        let mut capped = false;
        let mut turns: u64 = 0;
        loop {
            turns += 1;
            self.read_input();
            let turn = match self.turn(turns, capped).await {
                Some(Ok(turn)) => turn,
                Some(Err(failure)) => break Ending::Failed(failure),
                None => break Ending::Stopped,
            };
            self.shared.meter.add(turn.usage);
            if capped || turn.calls.is_empty() {
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
            self.budget.evict(&mut self.messages);
            // The whole prompt fills the window, cache reads included.
            if self.budget.reached(turn.usage.prompt()) {
                capped = true;
                self.cap_reached(turns, turn.usage.prompt());
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
            child.called();
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

#[path = "loop/attach.rs"]
mod attach;
#[path = "loop/children.rs"]
mod children;
#[path = "loop/conversation.rs"]
mod conversation;
#[path = "loop/finish.rs"]
mod finish;
#[path = "loop/model_turn.rs"]
mod model_turn;
#[path = "loop/shared.rs"]
pub(crate) mod shared;

#[cfg(test)]
#[path = "loop/tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "loop/image_tests.rs"]
mod image_tests;

#[cfg(test)]
#[path = "loop/budget_tests.rs"]
mod budget_tests;

#[cfg(test)]
#[path = "loop/history_tests.rs"]
mod history_tests;

#[cfg(test)]
#[path = "loop/provider_failure_tests.rs"]
mod provider_failure_tests;

#[cfg(test)]
#[path = "loop/turn_tests.rs"]
mod turn_tests;

#[cfg(test)]
#[path = "loop/end_tests.rs"]
mod end_tests;

#[cfg(test)]
#[path = "loop/memory_tests.rs"]
mod memory_tests;

#[cfg(test)]
#[path = "loop/session_tests.rs"]
mod session_tests;

#[cfg(test)]
#[path = "loop/checkout_tests.rs"]
mod checkout_tests;
