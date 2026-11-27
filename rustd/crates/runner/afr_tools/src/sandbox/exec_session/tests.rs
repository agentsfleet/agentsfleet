#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use af_core::clock::saturating_millis;
use af_core::test_util::trace::Capture;
use af_guest_wire::Ending;
use afr_guest_client::{ProcessId, Spawn};
use bytes::Bytes;
use serde_json::{Value, json};
use tokio::time::Instant;

use super::{EMPTY_WRITE_YIELD_MIN, YIELD_DEFAULT, session};
use crate::catalog::{EXEC_COMMAND, Entry, WRITE_STDIN};
use crate::run::Run;
use crate::runtime::ToolErrorCode;
use crate::sandbox::{ScriptedGuest, ScriptedProcess};
use crate::testing::call_in;
use guffbolt::tools::Limits;
use guffbolt::tools::process::oneshot::EVENT_OUT_OF_MEMORY;
use guffbolt::tools::process::sessions::SESSIONS_PER_RUN_MAX;

/// The id the scripted guest agent gives its first process.
pub(super) const FIRST: ProcessId = ProcessId::new(1);
/// The argument naming a session's command.
const CMD: &str = "cmd";
/// The argument naming the session a write goes to.
pub(super) const SESSION_ID: &str = "session_id";
/// The argument carrying what a write sends.
pub(super) const CHARS: &str = "chars";
/// The argument naming how long a call waits for output.
const YIELD_TIME_MS: &str = "yield_time_ms";
/// A command a session keeps running.
const DEV_SERVER: &str = "npm run dev";
/// The command that prints back what it is written.
pub(super) const CAT: &str = "cat";
/// An interactive program a session keeps open.
pub(super) const REPL: &str = "python3";
/// The directory a session asks to start in.
const WORKDIR: &str = "src";
/// What a REPL is told to leave by.
pub(super) const EXIT: &str = "exit()\n";
/// The first line written to a session.
pub(super) const ONE: &str = "one\n";
/// The second line written to a session.
const TWO: &str = "two\n";
/// How the first process's session reads while it runs.
pub(super) const FIRST_RUNNING: &str = "Process running with session ID 1";
/// The id of a run whose log lines a test reads: a default run's id is
/// empty, which a line that never read it would carry as well.
pub(super) const RUN_ID: &str = "run-oom";

/// A run named [`RUN_ID`], so a log line carries its id only by reading
/// it off the run.
pub(super) fn named_run() -> Run<'static> {
    Run {
        run_id: RUN_ID,
        ..Run::default()
    }
}

pub(super) fn exec() -> &'static Entry {
    &EXEC_COMMAND
}

pub(super) fn write() -> &'static Entry {
    &WRITE_STDIN
}

/// `exec_command`'s arguments for `cmd`, waiting `yield_ms` when given.
pub(super) fn opening(cmd: &str, yield_ms: Option<u64>) -> Value {
    json!({CMD: cmd, YIELD_TIME_MS: yield_ms})
}

/// `write_stdin`'s arguments: `chars` to session `id`, waiting `yield_ms`
/// when given.
pub(super) fn writing(id: u64, chars: &str, yield_ms: Option<u64>) -> Value {
    json!({SESSION_ID: id, CHARS: chars, YIELD_TIME_MS: yield_ms})
}

/// The one spawn `agent` was asked for.
fn only_spawn(agent: &ScriptedGuest) -> Spawn {
    match agent.spawned().as_slice() {
        [spawn] => spawn.clone(),
        other => panic!("one spawn, got {other:?}"),
    }
}

/// Opens one session on `agent` that yields as briefly as it may.
pub(super) async fn open(agent: &ScriptedGuest, run: &Run<'_>, cmd: &str) {
    call_in(
        exec(),
        agent,
        run,
        opening(cmd, Some(saturating_millis(Limits::DEFAULT.min_yield))),
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn test_exec_command_keeps_a_running_process_as_a_session() {
    let agent = ScriptedGuest::new([ScriptedProcess::stays_open("ready\n")]);
    let run = Run::default();
    let started = Instant::now();

    let output = call_in(exec(), &agent, &run, opening(DEV_SERVER, None)).await;

    assert_eq!(output.text, format!("ready\n{FIRST_RUNNING}"));
    assert_eq!((output.exit_code, output.error_code), (None, None));
    assert_eq!(
        started.elapsed(),
        YIELD_DEFAULT,
        "a running process is read for the whole yield"
    );
    assert!(run.sessions.get(session(FIRST)).is_some(), "it stays open");
    let spawn = only_spawn(&agent);
    assert_eq!(spawn.argv(), ["/bin/sh", "-c", DEV_SERVER]);
    assert!(!spawn.on_terminal(), "pipes unless the model asks");
    assert_eq!(spawn.time_limit(), None, "a session runs until it ends");
    assert_eq!(spawn.working_directory(), None);
    assert_eq!(spawn.environment().len(), 13);
}

#[tokio::test(start_paused = true)]
async fn should_start_on_a_terminal_in_a_workdir_when_asked() {
    let agent = ScriptedGuest::new([ScriptedProcess::exits("", 0)]);
    let arguments = json!({CMD: REPL, "tty": true, "workdir": WORKDIR});

    call_in(exec(), &agent, &Run::default(), arguments).await;

    let spawn = only_spawn(&agent);
    assert!(spawn.on_terminal());
    assert_eq!(spawn.working_directory(), Some(WORKDIR));
}

#[tokio::test(start_paused = true)]
async fn should_answer_at_once_with_the_exit_of_a_process_that_ends_in_its_yield() {
    let agent = ScriptedGuest::new([ScriptedProcess::exits("built\n", 2)]);
    let run = Run::default();
    let started = Instant::now();

    let output = call_in(exec(), &agent, &run, opening("make", None)).await;

    assert_eq!(output.text, "built\nProcess exited with code 2");
    assert_eq!(output.exit_code, Some(2));
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert!(
        run.sessions.get(session(FIRST)).is_none(),
        "no session is left"
    );
}

#[tokio::test(start_paused = true)]
async fn a_session_killed_for_memory_is_logged_as_a_one_shot_command_is() {
    let capture = Capture::install();
    let agent = ScriptedGuest::new([ScriptedProcess::ends("allocating\n", Ending::OutOfMemory)]);
    let run = named_run();

    let output = call_in(exec(), &agent, &run, opening(REPL, None)).await;

    assert_eq!(output.error_code, Some(ToolErrorCode::OutOfMemory.into()));
    let logged = capture.only(EVENT_OUT_OF_MEMORY);
    assert_eq!(logged.level, tracing::Level::WARN);
    assert_eq!(logged.field("error_code"), Some("out_of_memory"));
    assert_eq!(logged.field("run_id"), Some(RUN_ID));
}

#[tokio::test(start_paused = true)]
async fn should_hold_a_yield_between_the_shortest_and_the_longest() {
    let agent = ScriptedGuest::new([
        ScriptedProcess::stays_open(""),
        ScriptedProcess::stays_open(""),
    ]);
    let run = Run::default();

    let started = Instant::now();
    call_in(exec(), &agent, &run, opening("a", Some(1))).await;
    let shortest = started.elapsed();
    let started = Instant::now();
    call_in(exec(), &agent, &run, opening("b", Some(3_600_000))).await;
    let longest = started.elapsed();

    assert_eq!(shortest, Limits::DEFAULT.min_yield);
    assert_eq!(longest, Limits::DEFAULT.max_yield);
}

/// Past the cap a new session opens, and the least recently used one is
/// killed to make room.
#[tokio::test(start_paused = true)]
async fn test_exec_command_makes_room_past_the_session_cap() {
    let agent =
        ScriptedGuest::new((0..=SESSIONS_PER_RUN_MAX).map(|_| ScriptedProcess::stays_open("")));
    let run = Run::default();
    for _ in 0..SESSIONS_PER_RUN_MAX {
        open(&agent, &run, "sleep 600").await;
    }

    let opened = call_in(exec(), &agent, &run, opening("one more", None)).await;

    assert_eq!(opened.error_code, None, "{opened:?}");
    assert_eq!(agent.spawned().len(), SESSIONS_PER_RUN_MAX + 1);
    assert_eq!(agent.killed(), [FIRST], "the least recently used goes");
}

#[tokio::test(start_paused = true)]
async fn test_write_stdin_feeds_a_session_and_reads_what_it_printed() {
    let agent = ScriptedGuest::new([ScriptedProcess::echoes()]);
    let run = Run::default();
    open(&agent, &run, CAT).await;

    let first = call_in(write(), &agent, &run, writing(1, ONE, None)).await;
    let second = call_in(write(), &agent, &run, writing(1, TWO, None)).await;

    assert_eq!(first.text, format!("{ONE}{FIRST_RUNNING}"));
    assert_eq!(second.text, format!("{TWO}{FIRST_RUNNING}"));
    assert_eq!(
        agent.written(),
        [
            (FIRST, Bytes::from_static(ONE.as_bytes())),
            (FIRST, Bytes::from_static(TWO.as_bytes())),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn should_poll_with_an_empty_write_for_at_least_five_seconds() {
    let agent = ScriptedGuest::new([ScriptedProcess::stays_open("")]);
    let run = Run::default();
    open(&agent, &run, "tail -f build.log").await;
    let started = Instant::now();

    let polled = call_in(
        write(),
        &agent,
        &run,
        writing(1, "", Some(saturating_millis(Limits::DEFAULT.min_yield))),
    )
    .await;

    assert_eq!(started.elapsed(), EMPTY_WRITE_YIELD_MIN);
    assert_eq!(polled.text, FIRST_RUNNING);
    assert!(agent.written().is_empty(), "an empty write sends nothing");
}

#[tokio::test(start_paused = true)]
async fn should_answer_the_ending_of_a_process_that_exited_between_calls() {
    let agent = ScriptedGuest::new([ScriptedProcess::stays_open("")]);
    let run = Run::default();
    open(&agent, &run, "sh confirm.sh").await;
    assert!(agent.end(FIRST, Ending::Exited(4)));
    let answer = writing(1, "y\n", None);

    let output = call_in(write(), &agent, &run, answer.clone()).await;
    let again = call_in(write(), &agent, &run, answer).await;

    assert_eq!(output.text, "Process exited with code 4");
    assert_eq!(output.exit_code, Some(4));
    assert!(
        agent.written().is_empty(),
        "nothing written to an ended process"
    );
    assert_eq!(
        again.error_code,
        Some(ToolErrorCode::SessionNotFound.into())
    );
}

#[tokio::test(start_paused = true)]
async fn should_answer_the_exit_of_a_process_that_ends_after_a_write() {
    let agent = Arc::new(ScriptedGuest::new([ScriptedProcess::echoes()]));
    let run = Run::default();
    open(&agent, &run, REPL).await;
    let exiting = Arc::clone(&agent);
    // The process exits a moment after it reads what was written.
    let exit = tokio::spawn(async move {
        tokio::time::sleep(Limits::DEFAULT.min_yield).await;
        exiting.end(FIRST, Ending::Exited(0))
    });
    let started = Instant::now();

    let output = call_in(write(), &*agent, &run, writing(1, EXIT, None)).await;

    assert!(exit.await.unwrap(), "the process was open to end");
    assert_eq!(output.text, format!("{EXIT}Process exited with code 0"));
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(
        started.elapsed(),
        Limits::DEFAULT.min_yield,
        "answered as the process ended, not at the yield"
    );
    assert!(
        run.sessions.get(session(FIRST)).is_none(),
        "an ended session leaves"
    );
}

/// A process whose output was still open when it ended says so after its
/// output and before its status.
#[tokio::test(start_paused = true)]
async fn should_say_the_output_was_still_open_before_the_status_line() {
    let agent = ScriptedGuest::new([ScriptedProcess::ends_abandoned("out\n", Ending::Exited(0))]);

    let output = call_in(exec(), &agent, &Run::default(), opening(DEV_SERVER, None)).await;

    assert_eq!(
        output.text,
        "out\n... the process ended with its output still open; what was written after is not shown ...\nProcess exited with code 0"
    );
    assert_eq!((output.exit_code, output.error_code), (Some(0), None));
}
