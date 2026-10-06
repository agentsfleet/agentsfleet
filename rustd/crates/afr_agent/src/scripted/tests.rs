#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::sync::mpsc;

use afd_wire::activity::ActivityFrame;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::MemoryDelta;
use afd_wire::report::ResultOutcome;
use afr_egress::testing::CountingMint;
use afr_executor::{
    Ending, Events, Executor, FileContent, Listing, Process, ProcessId, Spawn, Stream,
};
use bytes::Bytes;
use tokio_util::sync::CancellationToken;

use super::{ScriptedEngine, Step};
use crate::engine::{AgentEngine, AgentRun, Meter};
use crate::testing::Discard;

/// A lease as the daemon spells one, trimmed to what a run reads.
const LEASE: &str = include_str!("lease.json");

/// An executor whose every process prints `out` and then ends as `ending`, or
/// closes its channel without an end when `ending` is `None`.
#[derive(Debug)]
struct Canned {
    ending: Option<Ending>,
    refuse: bool,
}

#[async_trait::async_trait]
impl Executor for Canned {
    async fn spawn(&self, _spawn: &Spawn) -> afr_executor::Result<Process> {
        if self.refuse {
            return Err(std::io::Error::other("the socket is gone").into());
        }
        let (feed, events) = Events::channel();
        feed.output(Stream::Stdout, Bytes::from_static(b"out"));
        // With no ending, the feed goes here, as a lost executor's does.
        if let Some(ending) = self.ending {
            feed.end(ending);
        }
        Ok(Process {
            id: ProcessId::new(1),
            events,
        })
    }
    async fn write(&self, _process: ProcessId, _data: Bytes) -> afr_executor::Result<()> {
        Ok(())
    }
    async fn kill(&self, _process: ProcessId) -> afr_executor::Result<()> {
        Ok(())
    }
    async fn read_file(&self, _path: &str, _max: u64) -> afr_executor::Result<FileContent> {
        Err(std::io::Error::other("unused").into())
    }
    async fn write_file(&self, _path: &str, _data: Bytes) -> afr_executor::Result<()> {
        Ok(())
    }
    async fn append_file(&self, _path: &str, _data: Bytes) -> afr_executor::Result<()> {
        Ok(())
    }
    async fn delete_file(&self, _path: &str) -> afr_executor::Result<()> {
        Ok(())
    }
    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Listing> {
        Ok(Listing::default())
    }
}

/// Runs `engine` against [`LEASE`], returning its output and every frame.
async fn drive(
    engine: &ScriptedEngine,
    executor: Option<&dyn Executor>,
) -> (crate::Result<crate::RunOutput>, Vec<ActivityFrame<'static>>) {
    let lease: LeasePayload<'_> = serde_json::from_str(LEASE).unwrap();
    let (frames, received) = mpsc::channel();
    let sink = move |frame: ActivityFrame<'static>| frames.send(frame).unwrap();
    let output = engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor,
            mint: &CountingMint::never(),
            checkpoint: &Discard,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await;
    drop(sink);
    (output, received.into_iter().collect())
}

fn echo() -> Step {
    Step::Run {
        tool: "shell",
        spawn: Spawn::program("echo").arg("hi"),
    }
}

#[tokio::test]
async fn a_clean_tool_call_reports_started_then_completed() {
    let executor = Canned {
        ending: Some(Ending::Exited(0)),
        refuse: false,
    };
    let (output, frames) = drive(&ScriptedEngine::new([echo()]), Some(&executor)).await;

    assert!(matches!(
        output.unwrap().result.outcome,
        ResultOutcome::Completed(_)
    ));
    assert!(
        matches!(&frames[0], ActivityFrame::ToolCallStarted(started) if started.call_id.as_deref() == Some("call_0"))
    );
    assert!(matches!(&frames[1], ActivityFrame::ToolCallCompleted(done) if done.name == "shell"));
    assert_eq!(frames.len(), 2);
}

#[tokio::test]
async fn a_failing_or_vanished_process_fails_the_turn() {
    for ending in [Some(Ending::Exited(1)), None] {
        let executor = Canned {
            ending,
            refuse: false,
        };
        let (output, _) = drive(&ScriptedEngine::new([echo()]), Some(&executor)).await;

        let ResultOutcome::Failed(failure) = output.unwrap().result.outcome else {
            unreachable!("{ending:?} must fail the turn");
        };
        assert!(
            failure.detail.starts_with("shell ended"),
            "{}",
            failure.detail
        );
    }
}

#[tokio::test]
async fn a_process_step_without_a_sandbox_fails_the_turn() {
    let (output, frames) = drive(&ScriptedEngine::new([echo()]), None).await;

    let ResultOutcome::Failed(failure) = output.unwrap().result.outcome else {
        unreachable!("a step that needs a sandbox cannot complete without one");
    };
    assert!(
        failure.detail.contains("needs a sandbox"),
        "{}",
        failure.detail
    );
    assert!(frames.is_empty());
}

#[tokio::test]
async fn an_executor_that_cannot_spawn_is_an_engine_error() {
    let executor = Canned {
        ending: None,
        refuse: true,
    };
    let (output, _) = drive(&ScriptedEngine::new([echo()]), Some(&executor)).await;

    let error = output.err().map(|failure| failure.code());
    assert_eq!(error, Some(afd_core::error_code::INTERNAL_OPERATION_FAILED));
}

#[tokio::test]
async fn said_text_streams_in_order_and_becomes_the_answer() {
    let engine = ScriptedEngine::new([Step::Say("hel".to_owned()), Step::Say("lo".to_owned())]);
    let (output, frames) = drive(&engine, None).await;

    assert_eq!(output.unwrap().result.content, "hello");
    let chunks: Vec<_> = frames
        .iter()
        .map(|frame| match frame {
            ActivityFrame::FleetResponseChunk(chunk) => (chunk.stream_seq, chunk.stream_start),
            other => unreachable!("only chunks were said: {other:?}"),
        })
        .collect();
    assert_eq!(chunks, [(0, true), (1, false)]);
}

#[tokio::test]
async fn remembered_items_are_handed_back_for_the_push() {
    let delta = MemoryDelta {
        key: Cow::Borrowed("k"),
        content: Cow::Borrowed("c"),
        category: Cow::Borrowed("core"),
        visibility: afd_wire::memory::Visibility::Fleet,
    };
    let (output, frames) = drive(&ScriptedEngine::new([Step::Remember(delta.clone())]), None).await;

    assert_eq!(output.unwrap().memory, [delta]);
    assert!(frames.is_empty());
}

#[tokio::test]
async fn a_run_debugs_without_the_leases_secrets() {
    let lease: LeasePayload<'_> = serde_json::from_str(LEASE).unwrap();
    let sink = |_frame: ActivityFrame<'static>| {};
    let run = AgentRun {
        lease: &lease,
        memory: afr_memory::Seed::default(),
        executor: None,
        mint: &CountingMint::never(),
        checkpoint: &Discard,
        events: &sink,
        meter: &Meter::default(),
        stop: &CancellationToken::new(),
    };

    let rendered = format!("{run:?}");

    assert!(rendered.contains("lease-1"), "{rendered}");
    assert!(
        !rendered.contains("\"k\""),
        "the api key must never render: {rendered}"
    );
}

#[test]
fn a_script_needs_a_sandbox_only_when_it_runs_a_process() {
    let lease: LeasePayload<'_> = serde_json::from_str(LEASE).unwrap();
    let talks = ScriptedEngine::new([Step::Say("hi".to_owned())]);
    let runs = ScriptedEngine::new([Step::Say("hi".to_owned()), echo()]);

    assert!(!talks.admit(&lease.policy).unwrap().sandbox);
    assert!(runs.admit(&lease.policy).unwrap().sandbox);
}
