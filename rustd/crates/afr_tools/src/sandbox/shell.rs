//! `shell`: one command, run to its end inside the sandbox.
//!
//! `sh -c command` on pipes, in the workspace, under the executor's own
//! timeout (`oneshot`).

use schemars::JsonSchema;
use serde::Deserialize;

use super::oneshot::{run_to_end, timeout_of};
use super::{command, executor_of};
use crate::catalog::{Entry, SHELL};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolOutput};

/// `shell`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Command {
    /// The command, as `sh -c` runs it in the workspace.
    command: String,
    /// How long it may run, in milliseconds: 10000 when absent, at most
    /// 600000.
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// Runs one command to its end.
#[derive(Debug)]
pub(crate) struct Shell;

#[async_trait::async_trait]
impl Handler for Shell {
    const ENTRY: &'static Entry = &SHELL;
    const DESCRIPTION: &'static str = "Run one shell command inside the sandbox, in the \
        workspace, and read back its output and exit code. The command is killed once \
        timeout_ms passes. To keep a process running across calls, use exec_command.";
    type Arguments = Command;

    async fn run(&self, arguments: Command, context: ToolContext<'_, '_>) -> ToolOutput {
        let executor = match executor_of(&context) {
            Ok(executor) => executor,
            Err(refused) => return refused,
        };
        let timeout = timeout_of(arguments.timeout_ms);
        let lease_id = context.lease.lease_id;
        run_to_end(executor, command(&arguments.command), timeout, lease_id).await
    }
}

#[cfg(test)]
#[path = "shell/tests.rs"]
mod tests;
