//! `shell`: one command, run to its end inside the sandbox.
//!
//! `sh -c command` on pipes, in the workspace, under the executor's own
//! timeout: the executor kills the process group when it elapses, TERM then
//! KILL, and reports `timed_out`, so no timer runs here. The exit status rides
//! the output, and the ledger marks a non-zero one failed.

use std::time::Duration;

use afd_core::clock::saturating_millis;
use afr_executor::Ending;
use schemars::JsonSchema;
use serde::Deserialize;

use super::output::{self, Collected};
use super::{command, executor_of, unavailable};
use crate::catalog::{Entry, SHELL};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// How long a command runs when the model names no timeout: Codex's
/// `DEFAULT_EXEC_COMMAND_TIMEOUT_MS`.
const TIMEOUT_MS_DEFAULT: u64 = 10_000;
/// The longest a command runs, whatever the model asks: ten minutes.
const TIMEOUT_MS_MAX: u64 = 600_000;
/// The event a command killed at its timeout logs under.
const EVENT_TIMED_OUT: &str = "process_timed_out";

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
        let timeout = Duration::from_millis(
            arguments
                .timeout_ms
                .unwrap_or(TIMEOUT_MS_DEFAULT)
                .min(TIMEOUT_MS_MAX),
        );
        let spawn = command(&arguments.command).timeout(timeout);
        let mut process = match executor.spawn(&spawn).await {
            Ok(process) => process,
            Err(failure) => return unavailable(&failure),
        };
        let mut collected = Collected::default();
        let ending = collected.read_to_end(&mut process).await;
        let text = collected.text(output::budget(None));
        let text = match ending {
            Ending::Exited(0) => text,
            Ending::TimedOut => {
                timed_out(context.lease.egress.lease_id(), timeout);
                let after = saturating_millis(timeout);
                output::with_line(text, &format!("{} after {after} ms", output::TIMED_OUT))
            }
            Ending::Exited(_) | Ending::Signaled(_) | Ending::Interrupted => {
                output::with_line(text, &output::status(ending))
            }
        };
        ToolOutput {
            text,
            exit_code: output::exit_code(ending),
            error_code: output::error_code(ending),
        }
    }
}

/// Logs a command the executor killed at its timeout: the model reads the
/// code, the operator reads the lease and how long the command was given.
fn timed_out(lease_id: &str, timeout: Duration) {
    let error_code = ToolErrorCode::TimedOut.as_str();
    let timeout_ms = saturating_millis(timeout);
    let event = EVENT_TIMED_OUT;
    tracing::warn!(lease_id, error_code, timeout_ms, event);
}

#[cfg(test)]
#[path = "shell/tests.rs"]
mod tests;
