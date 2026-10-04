//! The agent loop: turns until the model answers without a tool call, the
//! context cap is reached, the lease is stopped, or the provider fails.
//!
//! The [`Ledger`] keeps what each call did; this module runs the turns.
//! `docs/architecture/runner_execution.md` §"Tool catalog" is the design.

use std::time::Instant;

use afd_core::clock::SystemClock;
use afd_wire::memory::MemoryDelta;
use afd_wire::policy::ExecutionPolicy;
use afr_egress::Egress;
use afr_memory::Hydrated;
use afr_providers::{Call, Connect, Message, Provider, Replay, Request};
use afr_secrets::Scrub;
use afr_tools::{Catalog, Lease, Selection, ToolSpec};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

use crate::context::{Budget, CAP_REACHED, Checkpoints};
use crate::engine::{AgentEngine, AgentRun, Checkpoint, Meter, Needs, RunOutput};
use crate::error::Result;
use crate::events::Live;
use crate::ledger::Ledger;
use crate::prompt::Prompt;
use crate::router::{self, Router};
use crate::spans;
use crate::turn::{Turn, take};

const EVENT_CAP_REACHED: &str = "context_cap_reached";
const EVENT_CHECKPOINT_FAILED: &str = "memory_checkpoint_failed";
const EVENT_TURN_STARTED: &str = "provider_turn_started";
const EVENT_TURN_COMPLETED: &str = "provider_turn_completed";
const EVENT_PROVIDER_FAILED: &str = "provider_turn_failed";
/// Why a turn the lease stopped did not complete.
const REASON_STOPPED: &str = "lease_stopped";

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
        let harness = Harness::new(&run, &selection, &scrub);
        Ok(harness.drive(provider.as_ref()).instrument(span).await)
    }
}

/// How a run's turns ended.
enum Ending {
    Answered(String),
    Failed(afr_providers::Error),
    Stopped,
}

/// One run in progress.
struct Harness<'run> {
    lease_id: &'run str,
    model: &'run str,
    stop: &'run CancellationToken,
    checkpoint: &'run dyn Checkpoint,
    checkpoints: Checkpoints,
    selection: &'run Selection<'run>,
    router: Router<'run>,
    specs: Vec<ToolSpec<'run>>,
    scrub: &'run Scrub,
    /// What every call of the lease shares, lent to one call at a time.
    lease: Lease<'run>,
    live: Live<'run>,
    ledger: Ledger<'run>,
    budget: Budget,
    instructions: String,
    messages: Vec<Message>,
    meter: &'run Meter,
    started: Instant,
}

impl<'run> Harness<'run> {
    fn new(run: &AgentRun<'run>, selection: &'run Selection<'run>, scrub: &'run Scrub) -> Self {
        let policy = &run.lease.policy;
        let prompt = Prompt::new(run.lease);
        let started = Instant::now();
        Self {
            lease_id: &run.lease.lease_id,
            model: &policy.context.model,
            stop: run.stop,
            checkpoint: run.checkpoint,
            checkpoints: Checkpoints::new(&policy.context),
            selection,
            router: Router::new(selection, run.executor),
            specs: selection.specs().collect(),
            scrub,
            lease: Lease::new(
                Box::new(Hydrated::new(run.memory)),
                Egress::new(&run.lease.lease_id, policy, run.mint, &SystemClock),
            ),
            live: Live::new(run.events, scrub, started),
            ledger: Ledger::new(&run.lease.lease_id, run.events, scrub),
            budget: Budget::new(&policy.context),
            instructions: scrub.clean(prompt.instructions).into_inner(),
            messages: vec![Message::User(scrub.clean(prompt.message).into_inner())],
            meter: run.meter,
            started,
        }
    }

    async fn drive(mut self, provider: &dyn Provider) -> RunOutput {
        let mut capped = false;
        let mut turns: u64 = 0;
        let ending = loop {
            turns += 1;
            let turn = match self.turn(provider, turns, capped).await {
                Some(Ok(turn)) => turn,
                Some(Err(failure)) => break Ending::Failed(failure),
                None => break Ending::Stopped,
            };
            self.meter.add(turn.usage);
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
            if self.budget.reached(turn.usage.input) {
                capped = true;
                self.cap_reached(turns, turn.usage.input);
            }
        };
        self.finish(ending)
    }

    /// Writes the memory stored so far back, when any is; a stopped lease
    /// does not wait for it. A push that fails is logged and the run goes on:
    /// the push before the report carries every entry again.
    async fn checkpoint(&self) {
        let pending: Vec<MemoryDelta<'static>> = (self.lease.memory.pending().into_iter())
            .map(MemoryDelta::into_owned)
            .collect();
        if pending.is_empty() {
            return;
        }
        let pushed = tokio::select! {
            biased;
            () = self.stop.cancelled() => return,
            pushed = self.checkpoint.push(pending) => pushed,
        };
        if let Err(failure) = pushed {
            let error_code = failure.code().as_str();
            let lease_id = self.lease_id;
            let event = EVENT_CHECKPOINT_FAILED;
            tracing::warn!(error_code, lease_id, event);
        }
    }

    /// One model turn, its start and its end logged as a pair
    /// (`docs/LOGGING_STANDARD.md` §4 rule 1, at `debug` because a run makes
    /// one per pass); `None` when the lease stopped it.
    async fn turn(
        &mut self,
        provider: &dyn Provider,
        number: u64,
        capped: bool,
    ) -> Option<afr_providers::Result<Turn>> {
        let lease_id = self.lease_id;
        let turn = number;
        let event = EVENT_TURN_STARTED;
        tracing::debug!(lease_id, turn, event);
        let request = Request {
            model: self.model,
            instructions: &self.instructions,
            messages: &self.messages,
            tools: if capped { &[] } else { &self.specs },
            hosted: if capped { &[] } else { self.selection.hosted() },
        };
        let span = spans::chat(self.model);
        let streamed = take(provider.stream(request), &mut self.live).instrument(span.clone());
        let taken = tokio::select! {
            biased;
            () = self.stop.cancelled() => None,
            taken = streamed => Some(taken),
        };
        match &taken {
            Some(Ok(done)) => {
                let input_tokens = done.usage.input;
                let output_tokens = done.usage.output;
                spans::spent(&span, input_tokens, output_tokens);
                let calls = done.calls.len();
                let event = EVENT_TURN_COMPLETED;
                tracing::debug!(lease_id, turn, input_tokens, output_tokens, calls, event);
            }
            Some(Err(failure)) => {
                let code = failure.code().as_str();
                let event = EVENT_PROVIDER_FAILED;
                tracing::warn!(error_code = code, lease_id, turn, event);
            }
            None => {
                let reason = REASON_STOPPED;
                let event = EVENT_PROVIDER_FAILED;
                tracing::debug!(lease_id, turn, reason, event);
            }
        }
        taken
    }

    /// What the model said and called, as the conversation keeps it: scrubbed,
    /// so no secret value is ever sent to the model, whoever wrote it. The
    /// router ran each call with the arguments as the model wrote them. The
    /// provider's replay goes back unopened: it is the provider's own record
    /// of its reasoning, signed where the provider signs it.
    fn remembered(&self, text: String, calls: Vec<Call>, replay: Replay) -> Message {
        let calls = calls
            .into_iter()
            .map(|call| Call {
                arguments: self.scrub.clean_json(call.arguments).into_inner(),
                ..call
            })
            .collect();
        Message::Assistant {
            text: self.scrub.clean(text).into_inner(),
            calls,
            replay,
        }
    }

    /// Runs one call to its end, or answers it unrun when its turn was `cut`
    /// at the output limit; `None` when the lease stopped it.
    async fn call(&mut self, call: &Call, cut: bool) -> Option<Message> {
        let router = &self.router;
        let lease = &mut self.lease;
        let handler = async move {
            if cut {
                return router::cut(&call.name);
            }
            router.dispatch(&call.name, &call.arguments, lease).await
        };
        let text = tokio::select! {
            biased;
            () = self.stop.cancelled() => return None,
            text = self.ledger.call(call, handler) => text,
        };
        Some(Message::ToolResult {
            call_id: call.id.clone(),
            output: text.into_inner(),
        })
    }

    fn cap_reached(&mut self, turns: u64, tokens: u64) {
        let lease_id = self.lease_id;
        let event = EVENT_CAP_REACHED;
        tracing::info!(lease_id, turns, tokens, event);
        self.messages.push(Message::User(CAP_REACHED.to_owned()));
    }
}

#[path = "loop/finish.rs"]
mod finish;

#[cfg(test)]
#[path = "loop/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "loop/budget_tests.rs"]
mod budget_tests;

#[cfg(test)]
#[path = "loop/turn_tests.rs"]
mod turn_tests;

#[cfg(test)]
#[path = "loop/end_tests.rs"]
mod end_tests;

#[cfg(test)]
#[path = "loop/memory_tests.rs"]
mod memory_tests;
