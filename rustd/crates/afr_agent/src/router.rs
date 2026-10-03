//! The router: runs each call's handler where its runtime says.
//!
//! A supervisor-side handler runs in-process with no executor in its context;
//! a sandbox-side one runs with the lease's executor. A name the lease was not
//! offered is a tool error the model reads, never a failed run, so the next
//! turn still runs (`docs/architecture/runner_execution.md` §"Tool catalog").

use afr_executor::Executor;
use afr_tools::{Lease, Runtime, Selection, ToolContext, ToolErrorCode, ToolOutput};

/// What a call to a tool this run was not offered reads back.
const NOT_OFFERED: &str = "is not one of this run's tools";
/// What a call to a provider-hosted tool the provider did not run reads back.
const HOSTED_ELSEWHERE: &str = "runs at the model provider, and this provider offers none";
/// What a sandbox-side call on a run without a sandbox reads back.
const NO_SANDBOX: &str = "runs in the sandbox, and this run has none";
/// What a call from a turn cut at the output limit reads back.
const CUT: &str = "was not run: the turn reached the model's output limit and its \
                   arguments may be incomplete; call it again with shorter arguments";

/// Routes one run's tool calls.
#[derive(Debug)]
pub struct Router<'run> {
    selection: &'run Selection<'run>,
    executor: Option<&'run dyn Executor>,
}

impl<'run> Router<'run> {
    /// A router over the tools a lease was offered and, when it has one, its
    /// sandbox's executor.
    #[must_use]
    pub const fn new(
        selection: &'run Selection<'run>,
        executor: Option<&'run dyn Executor>,
    ) -> Self {
        Self {
            selection,
            executor,
        }
    }

    /// Runs one call to `name` with `arguments`, lending it the lease's state.
    pub async fn dispatch(
        &self,
        name: &str,
        arguments: &serde_json::Value,
        lease: &mut Lease<'_>,
    ) -> ToolOutput {
        let Some(tool) = self.selection.tool(name) else {
            return refused(self.selection.hosts(name), name);
        };
        let executor = match tool.runtime() {
            Runtime::Supervisor => None,
            Runtime::Sandbox => match self.executor {
                Some(executor) => Some(executor),
                None => return failed(ToolErrorCode::SandboxUnavailable, name, NO_SANDBOX),
            },
            Runtime::Provider => {
                return failed(ToolErrorCode::HostedToolUnavailable, name, HOSTED_ELSEWHERE);
            }
        };
        tool.call(arguments, ToolContext { executor, lease }).await
    }
}

/// The answer to a call from a turn the provider cut at its output limit:
/// its arguments may be incomplete, so it is never run.
pub(crate) fn cut(name: &str) -> ToolOutput {
    failed(ToolErrorCode::OutputLimitReached, name, CUT)
}

/// A call to a name with no handler in the lease's selection.
fn refused(hosted: bool, name: &str) -> ToolOutput {
    if hosted {
        failed(ToolErrorCode::HostedToolUnavailable, name, HOSTED_ELSEWHERE)
    } else {
        failed(ToolErrorCode::NotOffered, name, NOT_OFFERED)
    }
}

fn failed(code: ToolErrorCode, name: &str, reason: &str) -> ToolOutput {
    ToolOutput::failed(code, &format!("{name} {reason}"))
}

#[cfg(test)]
#[path = "router/tests.rs"]
mod tests;
