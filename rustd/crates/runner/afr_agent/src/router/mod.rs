//! The router: one loop's tools as `guffbolt` dispatches them, and each
//! catalog handler run where its runtime says.
//!
//! A supervisor-side handler runs in-process with no executor in its context;
//! a sandbox-side one runs with the run's executor, waiting for the run's
//! build when the sandbox is not ready yet. A name the loop holds no tool for
//! is a tool error the model reads, never a failed run, so the next turn
//! still runs (`docs/architecture/runner_execution.md` §"Tool catalog").

use std::future::ready;

use guffbolt::{CallRef, Describe, Tool, ToolFuture, ToolOutput, ToolSpec, Tools};
use tokio::time::Instant;

use afr_executor::{ExecutorSource, Outcome};
use afr_telemetry::labels::SandboxWait;
use afr_telemetry::record;
use afr_tools::{Run, RunTool, Runtime, Selection, ToolContext, ToolErrorCode};

use crate::harness::Harness;

/// What a call to a provider-hosted tool the provider did not run reads back.
const HOSTED_ELSEWHERE: &str = "runs at the model provider, and this provider offers none";

/// What a loop lends one call: the loop itself, and where the call sits in
/// its conversation.
#[derive(Clone, Copy)]
pub(crate) struct Lent<'call, 's, 'run> {
    /// The loop the call belongs to.
    pub(crate) harness: &'call Harness<'s, 'run>,
    /// Where the call sits, which a catalog handler is lent in turn.
    pub(crate) call_ref: &'call CallRef,
}

/// A tool one loop holds, lent the loop and the call's place for each call.
pub(crate) type LoopTool<'s, 'run> = dyn for<'call> Tool<Lent<'call, 's, 'run>> + 's;

/// Routes one loop's tool calls.
pub(crate) struct Router<'s, 'run> {
    tools: Tools<LoopTool<'s, 'run>>,
    /// What the loop was offered, for the provider-hosted names it holds no
    /// tool for.
    selection: &'s Selection<'run>,
}

impl<'s, 'run> Router<'s, 'run> {
    /// A router over `tools`, the loop's own, and the hosted entries
    /// `selection` offers.
    pub(crate) const fn new(
        tools: Tools<LoopTool<'s, 'run>>,
        selection: &'s Selection<'run>,
    ) -> Self {
        Self { tools, selection }
    }

    /// Offers `tool` after the loop's others.
    pub(crate) fn offer(&mut self, tool: Box<LoopTool<'s, 'run>>) {
        self.tools.offer(tool);
    }

    /// Holds `tool` for a call the model makes without being offered it.
    pub(crate) fn hold(&mut self, tool: Box<LoopTool<'s, 'run>>) {
        self.tools.hold(tool);
    }

    /// The functions the model is offered, in order.
    pub(crate) fn specs(&self) -> Vec<ToolSpec<'_>> {
        self.tools.specs().collect()
    }

    /// Runs one call to `name` with `arguments`, lending it `lent`. A
    /// provider-hosted name reached here because the provider ran no such
    /// tool, and is refused as the provider's.
    pub(crate) fn dispatch<'a>(
        &'a self,
        name: &'a str,
        arguments: &'a serde_json::Value,
        lent: Lent<'a, 's, 'run>,
    ) -> ToolFuture<'a> {
        if self.selection.hosts(name) {
            return Box::pin(ready(hosted_elsewhere(name)));
        }
        self.tools.dispatch(name, arguments, lent)
    }
}

/// A catalog handler, or a tool the run built over its backends, as a loop
/// runs it: lent the run's state, and its executor only when the handler's
/// published entry runs in the sandbox, the way Codex's
/// `ExtensionToolAdapter` lends an extension tool its turn.
#[derive(Debug)]
pub(crate) struct Routed<'s> {
    handler: &'s RunTool<'s>,
    runtime: Runtime,
}

impl<'s> Routed<'s> {
    /// `handler`, run where `runtime` says.
    pub(crate) const fn new(handler: &'s RunTool<'s>, runtime: Runtime) -> Self {
        Self { handler, runtime }
    }

    /// Runs the call at `call_ref` where the handler's runtime says: with
    /// `run`, and with the sandbox's executor from `executor` for a
    /// sandbox-side call.
    pub(crate) async fn run(
        &self,
        arguments: &serde_json::Value,
        executor: &dyn ExecutorSource,
        run: &Run<'_>,
        call_ref: &CallRef,
    ) -> ToolOutput {
        let name = self.handler.name();
        let executor = match self.runtime {
            Runtime::Supervisor => None,
            Runtime::Sandbox => match sandbox(executor).await {
                Ok(executor) => Some(executor),
                Err(why) => return failed(ToolErrorCode::SandboxUnavailable, name, why.reason()),
            },
            Runtime::Provider => return hosted_elsewhere(name),
        };
        self.handler
            .call(
                arguments,
                ToolContext {
                    executor,
                    run,
                    call_ref,
                },
            )
            .await
    }
}

impl Describe for Routed<'_> {
    fn spec(&self) -> ToolSpec<'_> {
        self.handler.spec()
    }
}

impl<'call, 's, 'run> Tool<Lent<'call, 's, 'run>> for Routed<'s> {
    fn call<'a>(
        &'a self,
        arguments: &'a serde_json::Value,
        cx: Lent<'call, 's, 'run>,
    ) -> ToolFuture<'a>
    where
        Lent<'call, 's, 'run>: 'a,
    {
        let shared = cx.harness.shared;
        Box::pin(self.run(arguments, shared.executor, shared.run, cx.call_ref))
    }
}

/// The sandbox's executor, or why there is none. A call that found the
/// build still running is recorded with how long it waited.
async fn sandbox(executor: &dyn ExecutorSource) -> Outcome<'_> {
    if executor.is_filled() {
        return executor.get().await;
    }
    let started = Instant::now();
    let outcome = executor.get().await;
    let waited = match outcome {
        Ok(_) => SandboxWait::Ready,
        Err(_) => SandboxWait::Unavailable,
    };
    record::sandbox_wait(waited, started.elapsed());
    outcome
}

/// A call to a tool the provider hosts, which this provider did not run.
fn hosted_elsewhere(name: &str) -> ToolOutput {
    failed(ToolErrorCode::HostedToolUnavailable, name, HOSTED_ELSEWHERE)
}

fn failed(code: ToolErrorCode, name: &str, reason: &str) -> ToolOutput {
    ToolOutput::failed(code, &format!("{name} {reason}"))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod wait_tests;
