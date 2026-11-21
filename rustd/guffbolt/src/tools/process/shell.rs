//! `shell`: one command, run to its end in the environment.
//!
//! `sh -c command` on pipes, in the working directory, under the environment's own
//! timeout (`oneshot`).

use std::time::Duration;

use crate::tool::ToolOutput;
use schemars::JsonSchema;
use serde::Deserialize;

use super::command;
use super::oneshot::{TIMEOUT_DEFAULT, run_to_end, timeout_of};
use crate::tools::env::Env;
use crate::tools::{Context, Handler};

/// `shell`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Command {
    /// The command, as `sh -c` runs it in the working directory.
    command: String,
    /// How long it may run, in milliseconds: 10000 when absent.
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// Runs one command to its end.
#[derive(Debug, Clone, Copy)]
pub struct Shell<'run> {
    backend: &'run dyn Env,
    /// How long a command runs when the model names no timeout.
    default_timeout: Duration,
}

impl<'run> Shell<'run> {
    /// The tool, running in `backend` for [`TIMEOUT_DEFAULT`] when the model
    /// names no timeout.
    #[must_use]
    pub const fn new(backend: &'run dyn Env) -> Self {
        Self {
            backend,
            default_timeout: TIMEOUT_DEFAULT,
        }
    }

    /// The same tool, a command running for `default_timeout` when the model
    /// names no timeout.
    #[must_use]
    pub const fn with_default_timeout(mut self, default_timeout: Duration) -> Self {
        self.default_timeout = default_timeout;
        self
    }
}

impl Handler for Shell<'_> {
    const NAME: &'static str = "shell";
    const DESCRIPTION: &'static str = "Run one shell command in the working directory and \
        read back its output and exit code. The command is killed once \
        timeout_ms passes. To keep a process running across calls, use exec_command.";
    type Arguments = Command;

    async fn run(&self, arguments: Command, context: Context<'_>) -> ToolOutput {
        let timeout = timeout_of(arguments.timeout_ms, self.default_timeout);
        let script = command(&arguments.command);
        run_to_end(self.backend, script, timeout, context.run_id).await
    }
}
