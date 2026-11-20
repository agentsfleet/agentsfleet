//! `shell`: one command, run to its end in the environment.
//!
//! `sh -c command` on pipes, in the working directory, under the environment's own
//! timeout (`oneshot`).

use crate::tool::ToolOutput;
use schemars::JsonSchema;
use serde::Deserialize;

use super::command;
use super::oneshot::{run_to_end, timeout_of};
use crate::tools::env::Env;
use crate::tools::{Context, Handler};

/// `shell`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Command {
    /// The command, as `sh -c` runs it in the working directory.
    command: String,
    /// How long it may run, in milliseconds: 10000 when absent, at most
    /// 600000.
    #[serde(default)]
    timeout_ms: Option<u64>,
}

backed! {
    /// Runs one command to its end.
    Shell over Env
}

impl Handler for Shell<'_> {
    const NAME: &'static str = "shell";
    const DESCRIPTION: &'static str = "Run one shell command in the working directory and \
        read back its output and exit code. The command is killed once \
        timeout_ms passes. To keep a process running across calls, use exec_command.";
    type Arguments = Command;

    async fn run(&self, arguments: Command, context: Context<'_>) -> ToolOutput {
        let timeout = timeout_of(arguments.timeout_ms);
        let script = command(&arguments.command);
        run_to_end(self.backend, script, timeout, context.run_id).await
    }
}
