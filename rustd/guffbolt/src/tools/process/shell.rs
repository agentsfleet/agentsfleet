//! `shell`: one command, run to its end inside the sandbox.
//!
//! `sh -c command` on pipes, in the workspace, under the environment's own
//! timeout (`oneshot`).

use crate::tool::ToolOutput;
use schemars::JsonSchema;
use serde::Deserialize;

use super::oneshot::{run_to_end, timeout_of};
use super::{command, env_of};
use crate::tools::{Context, Handler};

/// `shell`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Command {
    /// The command, as `sh -c` runs it in the workspace.
    command: String,
    /// How long it may run, in milliseconds: 10000 when absent, at most
    /// 600000.
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// Runs one command to its end.
#[derive(Debug)]
pub struct Shell;

impl Handler for Shell {
    const NAME: &'static str = "shell";
    const DESCRIPTION: &'static str = "Run one shell command inside the sandbox, in the \
        workspace, and read back its output and exit code. The command is killed once \
        timeout_ms passes. To keep a process running across calls, use exec_command.";
    type Arguments = Command;

    async fn run(&self, arguments: Command, context: Context<'_>) -> ToolOutput {
        let env = match env_of(&context) {
            Ok(env) => env,
            Err(refused) => return refused,
        };
        let timeout = timeout_of(arguments.timeout_ms);
        run_to_end(env, command(&arguments.command), timeout, context.run_id).await
    }
}
