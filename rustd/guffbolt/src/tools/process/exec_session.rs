//! `exec_command` and `write_stdin`: a process the model keeps across calls.
//!
//! Codex's unified exec, on the environment: `exec_command` starts `sh -c cmd`
//! and answers what arrived before its yield passed, leaving the process open
//! in the run's [`Sessions`] under the id the environment gave it; `write_stdin`
//! queues bytes to that process and yields the same way. A process that ended
//! leaves the registry with the answer that reports it, and the run's end
//! closes the rest. On a pseudo-terminal when the model asks for one, pipes
//! otherwise.

use std::time::Duration;

use bytes::Bytes;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::time::Instant;

use super::output::{self, Collected};
use super::sessions::Sessions;
use super::{command, env_of, oneshot, unavailable};
use crate::tool::ToolOutput;
use crate::tools::env::{Ending, ProcessId, Spawn};
use crate::tools::files::failed;
use crate::tools::{Context, Handler, code};

/// The shortest a call waits for output: Codex's `MIN_YIELD_TIME_MS`.
pub const YIELD_MIN: Duration = Duration::from_millis(250);
/// The longest a call waits for output: Codex's `MAX_YIELD_TIME_MS`.
pub const YIELD_MAX: Duration = Duration::from_secs(30);
/// How long a call waits when the model names no yield.
pub const YIELD_DEFAULT: Duration = Duration::from_secs(10);
/// The shortest an empty write waits, so polling a session is never a busy
/// loop: Codex's `MIN_EMPTY_YIELD_TIME_MS`.
pub const EMPTY_WRITE_YIELD_MIN: Duration = Duration::from_secs(5);
/// What a call naming no open session reads back after its id.
const NOT_OPEN: &str = "is not an open session";
/// What a write that found no process reads before the session's state: the
/// process had ended, and its ending follows.
pub const WRITE_UNDELIVERED: &str =
    "the write was not delivered: the process had already ended, and its ending follows";
/// What a write the process would not take reads before the session's state.
/// The environment's own sentence stays on the host: it is the
/// environment's to write, and the model's budget does not cover it.
pub const INPUT_REFUSED: &str =
    "the write was not taken: the process closed its input, or has not read what it was sent";
/// The event a write the process would not take logs under.
const EVENT_WRITE_REFUSED: &str = "exec_session_write_refused";

/// `exec_command`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Open {
    /// The command, as `sh -c` runs it.
    cmd: String,
    /// The directory to run it in, inside the workspace; the workspace itself
    /// when absent.
    #[serde(default)]
    workdir: Option<String>,
    /// Whether to run it on a pseudo-terminal, for a program that needs one;
    /// pipes when absent.
    #[serde(default)]
    tty: bool,
    /// How long to wait for output before answering, in milliseconds: 10000
    /// when absent, between 250 and 30000.
    #[serde(default)]
    yield_time_ms: Option<u64>,
    /// The most output tokens to read back: 10000 when absent, at most
    /// 262144.
    #[serde(default)]
    max_output_tokens: Option<usize>,
}

/// `write_stdin`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// The session id an earlier `exec_command` answered with.
    session_id: u64,
    /// What to write to the process's input; empty to only read what arrived.
    #[serde(default)]
    chars: String,
    /// How long to wait for output before answering, in milliseconds: 10000
    /// when absent, between 250 and 30000; an empty write waits at least 5000.
    #[serde(default)]
    yield_time_ms: Option<u64>,
    /// The most output tokens to read back: 10000 when absent, at most
    /// 262144.
    #[serde(default)]
    max_output_tokens: Option<usize>,
}

/// Opens a session.
#[derive(Debug)]
pub struct ExecCommand;

impl Handler for ExecCommand {
    const NAME: &'static str = "exec_command";
    const DESCRIPTION: &'static str = "Start a shell command inside the sandbox and keep it \
        running across calls. Answers with the output that arrived within yield_time_ms and, \
        while the process runs, its session ID; use write_stdin to send it input or read more. \
        The session ends when the process exits or the run ends.";
    type Arguments = Open;

    async fn run(&self, arguments: Open, context: Context<'_>) -> ToolOutput {
        let env = match env_of(&context) {
            Ok(env) => env,
            Err(refused) => return refused,
        };
        let sessions = context.sessions;
        let slot = sessions.make_room(env).await;
        let spawn = spawn_of(&arguments);
        let process = match env.spawn(&spawn).await {
            Ok(process) => process,
            Err(failure) => return failed(&failure),
        };
        let id = process.id;
        // Registered before it is read, so a call the lease stops mid-wait
        // still leaves the process where the run's end finds it.
        let shared = sessions.open(slot, process);
        let mut process = shared.lock().await;
        let mut collected = Collected::default();
        let deadline = Instant::now() + yield_of(arguments.yield_time_ms, YIELD_MIN);
        let ended = collected.until(&mut process, deadline).await;
        let budget = output::budget(arguments.max_output_tokens);
        let lease_id = context.run_id;
        // `process` stays locked through the close inside `reply`: a call that
        // locked a finished process before then would find its ending already
        // taken and answer `Interrupted`.
        reply(sessions, lease_id, id, &collected, ended, budget, None)
    }
}

/// Feeds a session.
#[derive(Debug)]
pub struct WriteStdin;

impl Handler for WriteStdin {
    const NAME: &'static str = "write_stdin";
    const DESCRIPTION: &'static str = "Write to a session exec_command started and read back \
        what it printed within yield_time_ms. Send empty chars to only read. Answers the exit \
        code once the process has ended.";
    type Arguments = Input;

    async fn run(&self, arguments: Input, context: Context<'_>) -> ToolOutput {
        let env = match env_of(&context) {
            Ok(env) => env,
            Err(refused) => return refused,
        };
        let id = ProcessId::new(arguments.session_id);
        let sessions = context.sessions;
        let Some(shared) = sessions.get(id) else {
            return not_open(id);
        };
        // Held across the yield: a second call on this session waits its
        // turn, and a call on another session never waits on this one.
        let mut process = shared.lock().await;
        let mut collected = Collected::default();
        let mut refused: Option<&'static str> = None;
        // A process that ended since the last call answers with its ending,
        // never with a write the environment would refuse.
        let ended = if let Some(ending) = collected.arrived(&mut process) {
            Some(ending)
        } else {
            let floor = if arguments.chars.is_empty() {
                EMPTY_WRITE_YIELD_MIN
            } else {
                match env.write(id, Bytes::from(arguments.chars)).await {
                    Ok(()) => YIELD_MIN,
                    // Ended between the look above and the write: its ending
                    // is on its way, behind whatever it left to say. The call
                    // waits its yield for it and reads the session as running
                    // until it lands; an environment that is gone ends the events.
                    Err(failure) if failure.is_unknown_process() => {
                        refused = Some(WRITE_UNDELIVERED);
                        YIELD_MIN
                    }
                    // The process would not take it: it closed its input, or
                    // has not read what it was sent. It runs on.
                    Err(failure) if failure.is_input_refused() => {
                        let session_id = id.get();
                        let event = EVENT_WRITE_REFUSED;
                        tracing::debug!(session_id, event);
                        refused = Some(INPUT_REFUSED);
                        YIELD_MIN
                    }
                    Err(failure) => return unavailable(&failure),
                }
            };
            let deadline = Instant::now() + yield_of(arguments.yield_time_ms, floor);
            collected.until(&mut process, deadline).await
        };
        let budget = output::budget(arguments.max_output_tokens);
        let lease_id = context.run_id;
        // Locked through the close inside `reply`, as in `ExecCommand::run`.
        reply(sessions, lease_id, id, &collected, ended, budget, refused)
    }
}

/// The process `open` asks for.
fn spawn_of(open: &Open) -> Spawn {
    let spawn = command(&open.cmd);
    let spawn = match &open.workdir {
        Some(workdir) => spawn.cwd(workdir),
        None => spawn,
    };
    if open.tty { spawn.terminal() } else { spawn }
}

/// How long a call waits: what the model asked, or the default, held between
/// `floor` and the longest yield.
///
/// `asked` is the model's `yield_time_ms` argument, a count of milliseconds
/// because that is the name and unit the tool schema publishes; it becomes a
/// `Duration` here, the one place the count is read.
fn yield_of(asked: Option<u64>, floor: Duration) -> Duration {
    asked
        .map_or(YIELD_DEFAULT, Duration::from_millis)
        .clamp(floor, YIELD_MAX)
}

/// One call's answer on session `id` of lease `lease_id`: what arrived, why a
/// write was refused when it was, then the session's state. A process that
/// ended leaves the registry here, logged as a one-shot command is when the
/// kernel killed it for memory.
fn reply(
    sessions: &Sessions,
    lease_id: &str,
    id: ProcessId,
    collected: &Collected,
    ended: Option<Ending>,
    budget: usize,
    refused: Option<&str>,
) -> ToolOutput {
    let state = ended.map_or_else(
        || format!("{} {}", output::RUNNING, id.get()),
        |ending| {
            if ending == Ending::OutOfMemory {
                oneshot::out_of_memory(lease_id);
            }
            sessions.close(id, ending);
            output::status(ending)
        },
    );
    let text = collected.text(budget);
    let text = match refused {
        Some(why) => output::with_line(text, why),
        None => text,
    };
    ToolOutput {
        text: output::with_line(text, &state),
        exit_code: ended.and_then(Ending::exit_code),
        error_code: ended.and_then(output::error_code),
        image: None,
        asks: None,
    }
}

/// What a call naming session `id`, which is not open, reads back.
fn not_open(id: ProcessId) -> ToolOutput {
    ToolOutput::failed(
        code::SESSION_NOT_FOUND,
        &format!("session {} {NOT_OPEN}", id.get()),
    )
}
