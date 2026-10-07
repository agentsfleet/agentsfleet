//! `write_stdin` against a process that would not, or could not, take the
//! write: gone by then, its input closed, or a refusal that is the sandbox's.

use std::time::Duration;

use afr_executor::Ending;
use bytes::Bytes;
use tokio::time::Instant;

use super::tests::{CAT, EXIT, FIRST, FIRST_RUNNING, ONE, REPL, open, write, writing};
use super::{INPUT_REFUSED, WRITE_UNDELIVERED, YIELD_MS_MIN};
use crate::lease::Lease;
use crate::runtime::ToolErrorCode;
use crate::sandbox::{ScriptedExecutor, ScriptedProcess};
use crate::testing::call_in;

/// A process that ended between the look before a write and the write
/// itself: the write finds no process, and the call answers its ending,
/// never the sandbox gone.
#[tokio::test(start_paused = true)]
async fn should_answer_the_ending_of_a_process_gone_by_the_time_of_a_write() {
    let executor =
        ScriptedExecutor::new([ScriptedProcess::ends_when_written("", Ending::Exited(3))]);
    let lease = Lease::default();
    open(&executor, &lease, REPL).await;

    let output = call_in(&*write(), &executor, &lease, writing(1, EXIT, None)).await;

    assert_eq!(
        output.text,
        format!("{WRITE_UNDELIVERED}\nProcess exited with code 3")
    );
    assert_eq!(output.exit_code, Some(3));
    assert_eq!(output.error_code, None);
    assert_eq!(
        executor.written(),
        [(FIRST, Bytes::from_static(EXIT.as_bytes()))]
    );
    assert!(lease.sessions.get(FIRST).is_none(), "the session is closed");
}

/// A write that found no process, with no ending yet: the ending is on its
/// way, so the call waits its yield, reads the session as running, and the
/// next call takes the ending.
#[tokio::test(start_paused = true)]
async fn should_read_a_process_gone_by_a_write_as_running_until_its_ending() {
    let executor = ScriptedExecutor::new([ScriptedProcess::gone_when_written("")]);
    let lease = Lease::default();
    open(&executor, &lease, REPL).await;
    let started = Instant::now();

    let first = call_in(
        &*write(),
        &executor,
        &lease,
        writing(1, EXIT, Some(YIELD_MS_MIN)),
    )
    .await;
    assert!(executor.end(FIRST, Ending::Exited(0)));
    let next = call_in(&*write(), &executor, &lease, writing(1, "", None)).await;

    assert_eq!(started.elapsed(), Duration::from_millis(YIELD_MS_MIN));
    assert_eq!(first.text, format!("{WRITE_UNDELIVERED}\n{FIRST_RUNNING}"));
    assert_eq!(first.error_code, None);
    assert_eq!(next.text, "Process exited with code 0");
    assert!(lease.sessions.get(FIRST).is_none(), "closed by the ending");
}

/// A process that would not take the write says why, on the line before its
/// state, and runs on.
#[tokio::test(start_paused = true)]
async fn should_say_why_a_write_was_refused_and_run_on() {
    let executor = ScriptedExecutor::new([ScriptedProcess::closes_its_input("")]);
    let lease = Lease::default();
    open(&executor, &lease, CAT).await;

    let output = call_in(&*write(), &executor, &lease, writing(1, ONE, None)).await;

    assert_eq!(output.text, format!("{INPUT_REFUSED}\n{FIRST_RUNNING}"));
    assert_eq!(output.error_code, None);
    assert!(lease.sessions.get(FIRST).is_some(), "runs on");
}

/// A write refused for no reason of the process's own is the sandbox gone,
/// answered at once, the session left for the run's end.
#[tokio::test(start_paused = true)]
async fn should_answer_any_other_refusal_at_once_and_keep_the_session() {
    let executor = ScriptedExecutor::new([ScriptedProcess::refuses_writes("")]);
    let lease = Lease::default();
    open(&executor, &lease, CAT).await;
    let started = Instant::now();

    let output = call_in(&*write(), &executor, &lease, writing(1, ONE, None)).await;

    assert_eq!(started.elapsed(), Duration::ZERO, "nothing to wait for");
    assert_eq!(output.error_code, Some(ToolErrorCode::SandboxUnavailable));
    assert!(lease.sessions.get(FIRST).is_some());
}

#[tokio::test(start_paused = true)]
async fn should_read_a_write_to_a_process_the_executor_lost_as_running() {
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("")]);
    let lease = Lease::default();
    open(&executor, &lease, CAT).await;
    assert!(executor.forget(FIRST));

    let output = call_in(&*write(), &executor, &lease, writing(1, "x", None)).await;

    assert_eq!(output.error_code, None);
    assert!(
        output.text.ends_with(FIRST_RUNNING),
        "its ending is still to come, got {:?}",
        output.text
    );
    assert!(
        lease.sessions.get(FIRST).is_some(),
        "left for the run's end to close"
    );
}
