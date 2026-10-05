//! A run's end closes every session its calls left open, including one whose
//! call the lease stopped mid-yield.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afd_wire::activity::ActivityFrame;
use afd_wire::lease::LeasePayload;
use afd_wire::report::ResultOutcome;
use afd_wire::tool_trace::ToolCallStatus;
use afr_egress::testing::{CountingMint, RecordingTransport};
use afr_executor::ProcessId;
use afr_providers::Chunk;
use afr_tools::Catalog;
use afr_tools::catalog::EXEC_COMMAND;
use afr_tools::sandbox::{ScriptedExecutor, ScriptedProcess};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::Loop;
use super::finish::{DETAIL_STOPPED, EVENT_SESSIONS_INTERRUPTED};
use super::tests::completions;
use crate::engine::{AgentEngine, AgentRun, Meter, RunOutput};
use crate::fixture::{Frames, Script, call, lease, say, unbounded};
use crate::testing::Discard;

/// The shortest yield a session call may ask for.
const BRIEF_YIELD_MS: u64 = 250;
/// The argument naming a session's command.
const CMD: &str = "cmd";
/// The argument naming how long a call waits for output.
const YIELD_TIME_MS: &str = "yield_time_ms";
/// The log field the run's end counts its killed sessions in.
const SESSIONS_FIELD: &str = "sessions";
/// The model's first call.
const FIRST_CALL: &str = "c1";
/// A command a session keeps running.
const DEV_SERVER: &str = "npm run dev";
/// What the model answers once its sessions are running.
const ANSWER: &str = "both are running";

/// The model's call `id` opening a session on `cmd`, waiting `yield_ms` when
/// given.
fn opening(id: &str, cmd: &str, yield_ms: Option<u64>) -> Chunk {
    call(
        id,
        EXEC_COMMAND.name(),
        json!({CMD: cmd, YIELD_TIME_MS: yield_ms}),
    )
}

/// Runs `lease` on the hosted catalog with `executor` as its sandbox's, until
/// it ends or `stop` is cancelled.
async fn drive_in(
    script: &Script,
    lease: &LeasePayload<'_>,
    executor: &ScriptedExecutor,
    stop: &CancellationToken,
) -> (RunOutput, Vec<ActivityFrame<'static>>) {
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let frames = Frames::default();
    let sink = frames.sink();
    let run = AgentRun {
        lease,
        memory: afr_memory::Seed::default(),
        executor: Some(executor),
        mint: &CountingMint::never(),
        checkpoint: &Discard,
        events: &sink,
        meter: &Meter::default(),
        stop,
    };
    let output = engine.run(run).await.unwrap();
    (output, frames.taken())
}

/// A lease offering the session tools.
fn session_lease() -> LeasePayload<'static> {
    lease(&[EXEC_COMMAND.name()], unbounded())
}

/// Whether `capture` saw the run's end kill any session.
fn interrupted_any(capture: &Capture) -> bool {
    capture
        .events()
        .iter()
        .any(|event| event.field("event") == Some(EVENT_SESSIONS_INTERRUPTED))
}

#[tokio::test(start_paused = true)]
async fn test_sessions_close_at_run_end() {
    let capture = Capture::install();
    let script = Script::new([
        vec![
            opening(FIRST_CALL, DEV_SERVER, Some(BRIEF_YIELD_MS)),
            opening("c2", "tail -f build.log", Some(BRIEF_YIELD_MS)),
        ],
        vec![say(ANSWER)],
    ]);
    let executor = ScriptedExecutor::new([
        ScriptedProcess::stays_open("ready\n"),
        ScriptedProcess::stays_open(""),
    ]);

    let (output, frames) = drive_in(
        &script,
        &session_lease(),
        &executor,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(output.result.content, ANSWER);
    assert_eq!(
        completions(&frames),
        [
            ("1".to_owned(), ToolCallStatus::Succeeded),
            ("2".to_owned(), ToolCallStatus::Succeeded),
        ]
    );
    assert_eq!(executor.killed(), [ProcessId::new(1), ProcessId::new(2)]);
    let closed = capture.only(EVENT_SESSIONS_INTERRUPTED);
    assert_eq!(closed.level, tracing::Level::INFO);
    assert_eq!(closed.field(SESSIONS_FIELD), Some("2"));
    assert_eq!(closed.field("lease_id"), Some("lease-1"));
}

#[tokio::test(start_paused = true)]
async fn a_session_whose_call_the_lease_stopped_is_still_closed() {
    let capture = Capture::install();
    let script = Script::new([vec![opening(FIRST_CALL, DEV_SERVER, None)]]);
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("")]);
    let stop = CancellationToken::new();
    let stopping = stop.clone();
    // Fires inside the call's ten-second yield.
    let stopper = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(BRIEF_YIELD_MS)).await;
        stopping.cancel();
    });

    let (output, frames) = drive_in(&script, &session_lease(), &executor, &stop).await;
    stopper.await.unwrap();

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a stopped run is not a completed one");
    };
    assert_eq!(failure.detail, DETAIL_STOPPED);
    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Interrupted)]
    );
    assert_eq!(
        executor.killed(),
        [ProcessId::new(1)],
        "registered, so killed"
    );
    assert_eq!(
        capture
            .only(EVENT_SESSIONS_INTERRUPTED)
            .field(SESSIONS_FIELD),
        Some("1")
    );
}

#[tokio::test(start_paused = true)]
async fn a_run_whose_sessions_all_ended_kills_nothing() {
    let capture = Capture::install();
    let script = Script::new([
        vec![opening(FIRST_CALL, "make", Some(BRIEF_YIELD_MS))],
        vec![say("built")],
    ]);
    let executor = ScriptedExecutor::new([ScriptedProcess::exits("ok\n", 0)]);

    let (_output, frames) = drive_in(
        &script,
        &session_lease(),
        &executor,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Succeeded)]
    );
    assert!(executor.killed().is_empty());
    assert!(!interrupted_any(&capture));
}
