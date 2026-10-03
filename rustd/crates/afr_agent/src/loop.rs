//! The agent loop: turns until the model answers without a tool call, the
//! context cap is reached, the lease is stopped, or the provider fails.
//!
//! The [`Ledger`] keeps what each call did; this module runs the turns.
//! `docs/architecture/runner_execution.md` §"Tool catalog" is the design.

use std::fmt;
use std::time::Instant;

use afd_wire::policy::ExecutionPolicy;
use afd_wire::report::{Completed, ExecutionResult, Failure, ResultOutcome};
use afr_providers::{Call, Message, Provider, Request, Usage};
use afr_tools::{Catalog, Selection, ToolSpec};
use tokio_util::sync::CancellationToken;

use crate::context::{Budget, CAP_REACHED};
use crate::engine::{AgentEngine, AgentRun, Needs, RunOutput};
use crate::error::Result;
use crate::events::Live;
use crate::ledger::Ledger;
use crate::prompt::Prompt;
use crate::router::Router;
use crate::scrub::Scrub;
use crate::turn::take;

/// What a run stopped by its lease reports as its detail.
const DETAIL_STOPPED: &str = "the run was stopped before it finished";
const EVENT_CAP_REACHED: &str = "context_cap_reached";
const EVENT_PROVIDER_FAILED: &str = "provider_turn_failed";

/// Builds the provider a lease's policy names.
pub type Connect =
    dyn Fn(&ExecutionPolicy<'_>) -> afr_providers::Result<Box<dyn Provider>> + Send + Sync;

/// The agent engine that runs the model against the lease's tools.
pub struct Loop {
    catalog: Catalog,
    connect: Box<Connect>,
}

impl Loop {
    /// A loop hosting `catalog`'s handlers and reaching models through
    /// `connect`.
    #[must_use]
    pub fn new(
        catalog: Catalog,
        connect: impl Fn(&ExecutionPolicy<'_>) -> afr_providers::Result<Box<dyn Provider>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            catalog,
            connect: Box::new(connect),
        }
    }
}

impl fmt::Debug for Loop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Loop")
            .field("catalog", &self.catalog)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl AgentEngine for Loop {
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<Needs> {
        let sandbox = self.catalog.select(&policy.tools)?.needs_sandbox();
        Ok(Needs { sandbox })
    }

    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput> {
        let policy = &run.lease.policy;
        let selection = self.catalog.select(&policy.tools)?;
        let provider = (self.connect)(policy)?;
        let scrub = Scrub::new(policy)?;
        let harness = Harness::new(&run, &selection, &scrub);
        Ok(harness.drive(provider.as_ref()).await)
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
    selection: &'run Selection<'run>,
    router: Router<'run>,
    specs: Vec<ToolSpec<'run>>,
    scrub: &'run Scrub,
    live: Live<'run>,
    ledger: Ledger<'run>,
    budget: Budget,
    instructions: String,
    messages: Vec<Message>,
    usage: Usage,
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
            selection,
            router: Router::new(selection, run.executor),
            specs: selection.specs().collect(),
            scrub,
            live: Live::new(run.events, scrub, started),
            ledger: Ledger::new(run.events, scrub),
            budget: Budget::new(&policy.context),
            instructions: prompt.instructions,
            messages: vec![Message::User(prompt.message)],
            usage: Usage::default(),
            started,
        }
    }

    async fn drive(mut self, provider: &dyn Provider) -> RunOutput {
        let mut capped = false;
        let mut turns: u64 = 0;
        let ending = loop {
            turns += 1;
            let request = Request {
                model: self.model,
                instructions: &self.instructions,
                messages: &self.messages,
                tools: if capped { &[] } else { &self.specs },
                hosted: if capped { &[] } else { self.selection.hosted() },
            };
            let turn = tokio::select! {
                biased;
                () = self.stop.cancelled() => break Ending::Stopped,
                turn = take(provider.stream(request), &mut self.live) => turn,
            };
            let turn = match turn {
                Ok(turn) => turn,
                Err(failure) => break Ending::Failed(failure),
            };
            self.usage += turn.usage;
            if capped || turn.calls.is_empty() {
                break Ending::Answered(turn.text);
            }
            let mut results = Vec::with_capacity(turn.calls.len());
            for call in &turn.calls {
                match self.call(call).await {
                    Some(result) => results.push(result),
                    None => break,
                }
            }
            self.messages.push(Message::Assistant {
                text: turn.text,
                calls: turn.calls,
            });
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

    /// Runs one call to its end; `None` when the lease stopped it.
    async fn call(&mut self, call: &Call) -> Option<Message> {
        let handler = self.router.dispatch(&call.name, &call.arguments);
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

    fn finish(self, ending: Ending) -> RunOutput {
        let (outcome, content) = match ending {
            Ending::Answered(text) => (
                ResultOutcome::Completed(Completed {}),
                self.scrub.text(&text).into_owned(),
            ),
            Ending::Failed(failure) => {
                let code = failure.code().as_str();
                let lease_id = self.lease_id;
                let event = EVENT_PROVIDER_FAILED;
                tracing::warn!(error_code = code, lease_id, event);
                let detail = failure.detail().into();
                (failed(failure.failure_class(), detail), String::new())
            }
            Ending::Stopped => (failed(None, DETAIL_STOPPED.into()), String::new()),
        };
        let (trace, records) = self.ledger.finish();
        RunOutput {
            result: ExecutionResult {
                outcome,
                content: content.into(),
                token_count: self.usage.total(),
                wall_seconds: self.started.elapsed().as_secs(),
                memory_peak_bytes: 0,
                cpu_throttled_ms: 0,
                input_tokens: self.usage.input,
                cached_input_tokens: self.usage.cached_input,
                output_tokens: self.usage.output,
            },
            memory: Vec::new(),
            trace,
            records,
        }
    }
}

fn failed(
    class: Option<afd_wire::report::FailureClass>,
    detail: std::borrow::Cow<'static, str>,
) -> ResultOutcome<'static> {
    ResultOutcome::Failed(Failure { class, detail })
}

#[cfg(test)]
#[path = "loop/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "loop/budget_tests.rs"]
mod budget_tests;

#[cfg(test)]
#[path = "loop/turn_tests.rs"]
mod turn_tests;
