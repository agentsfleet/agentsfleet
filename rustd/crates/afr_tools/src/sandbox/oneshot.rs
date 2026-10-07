//! A command run to its end under a timeout, as `shell` and `git` both run
//! theirs.
//!
//! The executor kills the process group when the timeout elapses, TERM then
//! KILL, and reports `timed_out`, so no timer runs here. The exit status rides
//! the output, and the ledger marks a non-zero one failed.

use std::time::Duration;

use afd_core::clock::saturating_millis;
use afr_executor::{Ending, Executor, Spawn};

use super::files::failed;
use super::output::{self, Collected};
use crate::runtime::{ToolErrorCode, ToolOutput};

/// How long a command runs when the model names no timeout: Codex's
/// `DEFAULT_EXEC_COMMAND_TIMEOUT_MS`.
pub(super) const TIMEOUT_MS_DEFAULT: u64 = 10_000;
/// The longest a command runs, whatever the model asks: ten minutes.
pub(super) const TIMEOUT_MS_MAX: u64 = 600_000;
/// The event a command killed at its timeout logs under.
pub(super) const EVENT_TIMED_OUT: &str = "process_timed_out";
/// The event a command the kernel killed for memory logs under.
pub(super) const EVENT_OUT_OF_MEMORY: &str = "sandbox_out_of_memory";

/// How long a command may run: what the model asked, or the default, never
/// past the ceiling.
pub(super) fn timeout_of(asked: Option<u64>) -> Duration {
    Duration::from_millis(asked.unwrap_or(TIMEOUT_MS_DEFAULT).min(TIMEOUT_MS_MAX))
}

/// Runs `spawn` to its end under `timeout`, for lease `lease_id`, and answers
/// with its output and a status line for any ending but a clean exit.
pub(super) async fn run_to_end(
    executor: &dyn Executor,
    spawn: Spawn,
    timeout: Duration,
    lease_id: &str,
) -> ToolOutput {
    let mut process = match executor.spawn(&spawn.timeout(timeout)).await {
        Ok(process) => process,
        Err(failure) => return failed(&failure),
    };
    let mut collected = Collected::default();
    let ending = collected.read_to_end(&mut process).await;
    let text = collected.text(output::budget(None));
    let text = match ending {
        Ending::Exited(0) => text,
        Ending::TimedOut => {
            timed_out(lease_id, timeout);
            let after = saturating_millis(timeout);
            output::with_line(text, &format!("{} after {after} ms", output::TIMED_OUT))
        }
        Ending::OutOfMemory => {
            out_of_memory(lease_id);
            output::with_line(text, &output::status(ending))
        }
        Ending::Exited(_) | Ending::Signaled(_) | Ending::Interrupted => {
            output::with_line(text, &output::status(ending))
        }
    };
    ToolOutput {
        text,
        exit_code: output::exit_code(ending),
        error_code: output::error_code(ending),
        image: None,
    }
}

/// Logs a command the kernel killed because the sandbox's tenant processes
/// ran out of memory: the model reads the code, the operator the lease. No
/// command text, which is the tenant's.
pub(super) fn out_of_memory(lease_id: &str) {
    let error_code = ToolErrorCode::OutOfMemory.as_str();
    let event = EVENT_OUT_OF_MEMORY;
    tracing::warn!(lease_id, error_code, event);
}

/// Logs a command the executor killed at its timeout: the model reads the
/// code, the operator reads the lease and how long the command was given.
fn timed_out(lease_id: &str, timeout: Duration) {
    let error_code = ToolErrorCode::TimedOut.as_str();
    let timeout_ms = saturating_millis(timeout);
    let event = EVENT_TIMED_OUT;
    tracing::warn!(lease_id, error_code, timeout_ms, event);
}
