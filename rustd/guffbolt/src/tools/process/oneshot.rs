//! A command run to its end under a timeout, as `shell` and `git` both run
//! theirs.
//!
//! The environment enforces the process timeout. The tool separately bounds
//! its wait for that ending, by the environment's own grace: an environment
//! can stay reachable while its process driver stops answering. Missing
//! endings remain unknown.

use std::time::Duration;

use tokio::time::Instant;

use super::output::{self, Collected};
use super::saturating_millis;
use crate::tool::{ErrorCode, ToolOutput};
use crate::tools::code;
use crate::tools::env::{Ending, Env, Process, Spawn};
use crate::tools::files::failed;

/// How long a command runs when the model names no timeout: Codex's
/// `DEFAULT_EXEC_COMMAND_TIMEOUT_MS`.
pub const TIMEOUT_DEFAULT: Duration = Duration::from_secs(10);
/// The longest a command runs, whatever the model asks: ten minutes.
pub const TIMEOUT_MAX: Duration = Duration::from_mins(10);
/// Missing endings differ from confirmed process timeouts.
pub const EVENT_ENDING_DEADLINE: &str = "process_ending_deadline_exceeded";
/// The event a command killed at its timeout logs under.
pub const EVENT_TIMED_OUT: &str = "process_timed_out";
/// The event a command the kernel killed for memory logs under.
pub const EVENT_OUT_OF_MEMORY: &str = "sandbox_out_of_memory";

/// How long a command may run: what the model asked, or the default, never
/// past the ceiling.
///
/// `asked` is the model's `timeout_ms` argument, a count of milliseconds
/// because that is the name and unit the tool schema publishes; it becomes a
/// `Duration` here, the one place the count is read.
pub(super) fn timeout_of(asked: Option<u64>) -> Duration {
    asked
        .map_or(TIMEOUT_DEFAULT, Duration::from_millis)
        .min(TIMEOUT_MAX)
}

/// Runs `spawn` to its end under `timeout`, for run `run_id`, and answers
/// with its output and a status line for any ending but a clean exit.
pub(super) async fn run_to_end(
    env: &dyn Env,
    spawn: Spawn,
    timeout: Duration,
    run_id: &str,
) -> ToolOutput {
    let spawn = spawn.timeout(timeout);
    let mut process = match env.spawn(&spawn).await {
        Ok(process) => process,
        Err(failure) => return failed(&failure),
    };
    let mut collected = Collected::default();
    let runner_wait = timeout.saturating_add(env.ending_grace());
    let ending = collected
        .until(&mut process, Instant::now() + runner_wait)
        .await
        .unwrap_or_else(|| missing_ending(&mut process, run_id, timeout, runner_wait));
    let text = collected.text(output::budget(None));
    let text = match ending {
        Ending::Exited(0) => text,
        Ending::TimedOut => {
            timed_out(run_id, timeout);
            let after = saturating_millis(timeout);
            output::with_line(text, &format!("{} after {after} ms", output::TIMED_OUT))
        }
        Ending::OutOfMemory => {
            out_of_memory(run_id);
            output::with_line(text, &output::status(ending))
        }
        Ending::Exited(_) | Ending::Signaled(_) | Ending::Interrupted => {
            output::with_line(text, &output::status(ending))
        }
    };
    let mut answered = ToolOutput::succeeded(text);
    answered.exit_code = ending.exit_code();
    answered.error_code = output::error_code(ending);
    answered
}

fn missing_ending(
    process: &mut Process,
    run_id: &str,
    timeout: Duration,
    runner_wait: Duration,
) -> Ending {
    process.events.disconnect();
    let error_code = ErrorCode::INTERRUPTED.as_str();
    let process_id = process.id.get();
    let timeout_ms = saturating_millis(timeout);
    let runner_wait_ms = saturating_millis(runner_wait);
    {
        let event = &EVENT_ENDING_DEADLINE;
        tracing::warn!(
            event,
            run_id,
            error_code,
            process_id,
            timeout_ms,
            runner_wait_ms
        );
    };
    Ending::Interrupted
}

/// Logs a command the kernel killed because the environment's processes ran
/// out of memory: the model reads the code, the operator the run. No command
/// text, which is the user's.
pub(super) fn out_of_memory(run_id: &str) {
    let error_code = code::OUT_OF_MEMORY.as_str();
    let event = EVENT_OUT_OF_MEMORY;
    tracing::warn!(run_id, error_code, event);
}

/// Logs a command the executor killed at its timeout: the model reads the
/// code, the operator reads the run and how long the command was given.
fn timed_out(run_id: &str, timeout: Duration) {
    let error_code = code::TIMED_OUT.as_str();
    let timeout_ms = saturating_millis(timeout);
    let event = EVENT_TIMED_OUT;
    tracing::warn!(run_id, error_code, timeout_ms, event);
}
