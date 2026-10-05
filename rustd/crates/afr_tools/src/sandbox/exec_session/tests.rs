#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afr_executor::{Ending, ProcessId, Spawn};
use bytes::Bytes;
use serde_json::{Value, json};
use tokio::time::Instant;

use super::{
    EMPTY_WRITE_YIELD_MS_MIN, ExecCommand, WriteStdin, YIELD_MS_DEFAULT, YIELD_MS_MAX, YIELD_MS_MIN,
};
use crate::handler::Typed;
use crate::lease::Lease;
use crate::runtime::{Tool, ToolErrorCode};
use crate::sandbox::sessions::SESSIONS_PER_LEASE_MAX;
use crate::sandbox::{ScriptedExecutor, ScriptedProcess};
use crate::testing::call_in;

/// The id the scripted executor gives its first process.
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
const REPL: &str = "python3";
/// The directory a session asks to start in.
const WORKDIR: &str = "src";
/// The first line written to a session.
const ONE: &str = "one\n";
/// The second line written to a session.
const TWO: &str = "two\n";
/// How the first process's session reads while it runs.
const FIRST_RUNNING: &str = "Process running with session ID 1";

pub(super) fn exec() -> Box<dyn Tool> {
    Typed::boxed(ExecCommand)
}

pub(super) fn write() -> Box<dyn Tool> {
    Typed::boxed(WriteStdin)
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

/// The one spawn `executor` was asked for.
fn only_spawn(executor: &ScriptedExecutor) -> Spawn {
    match executor.spawned().as_slice() {
        [spawn] => spawn.clone(),
        other => panic!("one spawn, got {other:?}"),
    }
}

/// Opens one session on `executor` that yields as briefly as it may.
pub(super) async fn open(executor: &ScriptedExecutor, lease: &mut Lease<'_>, cmd: &str) {
    call_in(&*exec(), executor, lease, opening(cmd, Some(YIELD_MS_MIN))).await;
}

#[tokio::test(start_paused = true)]
async fn test_exec_command_keeps_a_running_process_as_a_session() {
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("ready\n")]);
    let mut lease = Lease::default();
    let started = Instant::now();

    let output = call_in(&*exec(), &executor, &mut lease, opening(DEV_SERVER, None)).await;

    assert_eq!(output.text, format!("ready\n{FIRST_RUNNING}"));
    assert_eq!((output.exit_code, output.error_code), (None, None));
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(YIELD_MS_DEFAULT),
        "a running process is read for the whole yield"
    );
    assert!(lease.sessions.get_mut(FIRST).is_some(), "it stays open");
    let spawn = only_spawn(&executor);
    assert_eq!(spawn.argv(), ["/bin/sh", "-c", DEV_SERVER]);
    assert!(!spawn.on_terminal(), "pipes unless the model asks");
    assert_eq!(spawn.time_limit(), None, "a session runs until it ends");
    assert_eq!(spawn.working_directory(), None);
    assert_eq!(spawn.environment().len(), 13);
}

#[tokio::test(start_paused = true)]
async fn should_start_on_a_terminal_in_a_workdir_when_asked() {
    let executor = ScriptedExecutor::new([ScriptedProcess::exits("", 0)]);
    let arguments = json!({CMD: REPL, "tty": true, "workdir": WORKDIR});

    call_in(&*exec(), &executor, &mut Lease::default(), arguments).await;

    let spawn = only_spawn(&executor);
    assert!(spawn.on_terminal());
    assert_eq!(spawn.working_directory(), Some(WORKDIR));
}

#[tokio::test(start_paused = true)]
async fn should_answer_at_once_with_the_exit_of_a_process_that_ends_in_its_yield() {
    let executor = ScriptedExecutor::new([ScriptedProcess::exits("built\n", 2)]);
    let mut lease = Lease::default();
    let started = Instant::now();

    let output = call_in(&*exec(), &executor, &mut lease, opening("make", None)).await;

    assert_eq!(output.text, "built\nProcess exited with code 2");
    assert_eq!(output.exit_code, Some(2));
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert!(
        lease.sessions.get_mut(FIRST).is_none(),
        "no session is left"
    );
}

#[tokio::test(start_paused = true)]
async fn should_hold_a_yield_between_the_shortest_and_the_longest() {
    let executor = ScriptedExecutor::new([
        ScriptedProcess::stays_open(""),
        ScriptedProcess::stays_open(""),
    ]);
    let mut lease = Lease::default();

    let started = Instant::now();
    call_in(&*exec(), &executor, &mut lease, opening("a", Some(1))).await;
    let shortest = started.elapsed();
    let started = Instant::now();
    call_in(
        &*exec(),
        &executor,
        &mut lease,
        opening("b", Some(3_600_000)),
    )
    .await;
    let longest = started.elapsed();

    assert_eq!(shortest, Duration::from_millis(YIELD_MS_MIN));
    assert_eq!(longest, Duration::from_millis(YIELD_MS_MAX));
}

/// Past the cap a new session opens, and the least recently used one is
/// killed to make room, as Codex does.
#[tokio::test(start_paused = true)]
async fn test_exec_command_makes_room_past_the_session_cap() {
    let executor = ScriptedExecutor::new(
        (0..=SESSIONS_PER_LEASE_MAX).map(|_| ScriptedProcess::stays_open("")),
    );
    let mut lease = Lease::default();
    for _ in 0..SESSIONS_PER_LEASE_MAX {
        open(&executor, &mut lease, "sleep 600").await;
    }

    let opened = call_in(&*exec(), &executor, &mut lease, opening("one more", None)).await;

    assert_eq!(opened.error_code, None, "{opened:?}");
    assert_eq!(executor.spawned().len(), SESSIONS_PER_LEASE_MAX + 1);
    assert_eq!(executor.killed(), [FIRST], "the least recently used goes");
}

#[tokio::test(start_paused = true)]
async fn test_write_stdin_feeds_a_session_and_reads_what_it_printed() {
    let executor = ScriptedExecutor::new([ScriptedProcess::echoes()]);
    let mut lease = Lease::default();
    open(&executor, &mut lease, CAT).await;

    let first = call_in(&*write(), &executor, &mut lease, writing(1, ONE, None)).await;
    let second = call_in(&*write(), &executor, &mut lease, writing(1, TWO, None)).await;

    assert_eq!(first.text, format!("{ONE}{FIRST_RUNNING}"));
    assert_eq!(second.text, format!("{TWO}{FIRST_RUNNING}"));
    assert_eq!(
        executor.written(),
        [
            (FIRST, Bytes::from_static(ONE.as_bytes())),
            (FIRST, Bytes::from_static(TWO.as_bytes())),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn should_poll_with_an_empty_write_for_at_least_five_seconds() {
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("")]);
    let mut lease = Lease::default();
    open(&executor, &mut lease, "tail -f build.log").await;
    let started = Instant::now();

    let polled = call_in(
        &*write(),
        &executor,
        &mut lease,
        writing(1, "", Some(YIELD_MS_MIN)),
    )
    .await;

    assert_eq!(
        started.elapsed(),
        Duration::from_millis(EMPTY_WRITE_YIELD_MS_MIN)
    );
    assert_eq!(polled.text, FIRST_RUNNING);
    assert!(
        executor.written().is_empty(),
        "an empty write sends nothing"
    );
}

#[tokio::test(start_paused = true)]
async fn should_answer_the_ending_of_a_process_that_exited_between_calls() {
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("")]);
    let mut lease = Lease::default();
    open(&executor, &mut lease, "sh confirm.sh").await;
    assert!(executor.end(FIRST, Ending::Exited(4)));
    let answer = writing(1, "y\n", None);

    let output = call_in(&*write(), &executor, &mut lease, answer.clone()).await;
    let again = call_in(&*write(), &executor, &mut lease, answer).await;

    assert_eq!(output.text, "Process exited with code 4");
    assert_eq!(output.exit_code, Some(4));
    assert!(
        executor.written().is_empty(),
        "nothing written to an ended process"
    );
    assert_eq!(again.error_code, Some(ToolErrorCode::SessionNotFound));
}

#[tokio::test(start_paused = true)]
async fn should_answer_the_exit_of_a_process_that_ends_after_a_write() {
    let executor = Arc::new(ScriptedExecutor::new([ScriptedProcess::echoes()]));
    let mut lease = Lease::default();
    open(&executor, &mut lease, REPL).await;
    let exiting = Arc::clone(&executor);
    // The process exits a moment after it reads what was written.
    let exit = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(YIELD_MS_MIN)).await;
        exiting.end(FIRST, Ending::Exited(0))
    });
    let started = Instant::now();

    let output = call_in(
        &*write(),
        &*executor,
        &mut lease,
        writing(1, "exit()\n", None),
    )
    .await;

    assert!(exit.await.unwrap(), "the process was open to end");
    assert_eq!(output.text, "exit()\nProcess exited with code 0");
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(YIELD_MS_MIN),
        "answered as the process ended, not at the yield"
    );
    assert!(
        lease.sessions.get_mut(FIRST).is_none(),
        "an ended session leaves"
    );
}
