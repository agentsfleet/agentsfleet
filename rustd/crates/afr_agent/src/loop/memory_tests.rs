#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afd_wire::memory::{MemoryDelta, PINNED_CATEGORY};
use afr_egress::testing::{CountingMint, RecordingTransport};
use afr_providers::{Chunk, Message};
use afr_tools::Catalog;
use afr_tools::catalog::{MEMORY_FORGET, MEMORY_LIST, MEMORY_RECALL, MEMORY_STORE};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::{EVENT_CHECKPOINT_FAILED, Loop};
use crate::engine::{AgentEngine, AgentRun, Checkpoint, Meter, RunOutput};
use crate::fixture::{Frames, Script, call, lease, say, unbounded};
use crate::testing::{Discard, Recording};

/// What each call of the run read back, in call order.
fn read_back(messages: &[Message]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.as_str()),
            _other => None,
        })
        .collect()
}

#[tokio::test]
async fn test_memory_tools_round_trip_through_push() {
    let hydrated = [MemoryDelta {
        key: Cow::Borrowed("incident:41"),
        content: Cow::Borrowed("escalated to the on-call"),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: afd_wire::memory::Visibility::Fleet,
    }];
    let script = Script::new([
        vec![call(
            "m1",
            MEMORY_STORE.name(),
            json!({"key": "incident:42", "content": "deploy 812 broke iad", "category": "daily"}),
        )],
        vec![call(
            "m2",
            MEMORY_RECALL.name(),
            json!({"query": "incident"}),
        )],
        vec![call(
            "m3",
            MEMORY_FORGET.name(),
            json!({"key": "incident:41"}),
        )],
        vec![call("m4", MEMORY_LIST.name(), json!({}))],
        vec![say("incident 42 is new")],
    ]);
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let names = [MEMORY_STORE, MEMORY_RECALL, MEMORY_FORGET, MEMORY_LIST].map(|entry| entry.name());
    let lease = lease(&names, unbounded());
    let frames = Frames::default();
    let sink = frames.sink();

    let output = engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::window(&hydrated),
            executor: None,
            mint: &CountingMint::never(),
            checkpoint: &Discard,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();
    frames.taken();

    let sent = script.sent();
    let read = read_back(&sent[4].messages);
    assert_eq!(read[0], "stored incident:42");
    assert_eq!(
        read[1],
        "incident:42 (daily): deploy 812 broke iad\nincident:41 (core): escalated to the on-call",
        "a store is recalled in the same run, ahead of what was hydrated"
    );
    assert!(
        read[2].starts_with("incident:41 is forgotten"),
        "{}",
        read[2]
    );
    assert_eq!(
        read[3], "incident:42 (daily)",
        "a forget holds for the rest of the run"
    );
    assert_eq!(
        output.memory,
        [MemoryDelta {
            key: Cow::Borrowed("incident:42"),
            content: Cow::Borrowed("deploy 812 broke iad"),
            category: Cow::Borrowed("daily"),
            visibility: afd_wire::memory::Visibility::Fleet,
        }],
        "the push carries what the run stored, and nothing it only read"
    );
}

/// The keys each checkpoint carried, in push order.
fn keys<'p>(pushes: &'p [Vec<MemoryDelta<'static>>]) -> Vec<Vec<&'p str>> {
    pushes
        .iter()
        .map(|push| push.iter().map(|delta| delta.key.as_ref()).collect())
        .collect()
}

#[tokio::test]
async fn the_memory_is_checkpointed_every_n_calls() {
    let stores = ["k1", "k2", "k3", "k4", "k5"].map(|key| {
        vec![call(
            key,
            MEMORY_STORE.name(),
            json!({"key": key, "content": "x"}),
        )]
    });
    let script = Script::new(stores.into_iter().chain([vec![say("stored")]]));
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let every_two = json!({"tool_window": 0, "memory_checkpoint_every": 2,
        "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 0});
    let lease = lease(&[MEMORY_STORE.name()], every_two);
    let (checkpoint, pushed) = Recording::new();
    let frames = Frames::default();
    let sink = frames.sink();

    engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &CountingMint::never(),
            checkpoint: &checkpoint,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();

    frames.taken();
    let written: Vec<_> = pushed.try_iter().collect();
    assert_eq!(
        keys(&written),
        [vec!["k1", "k2"], vec!["k1", "k2", "k3", "k4"]],
        "every second call writes back all the run has stored; the fifth waits for the final push"
    );
}

/// Context knobs that checkpoint after every call.
fn every_call() -> serde_json::Value {
    json!({"tool_window": 0, "memory_checkpoint_every": 1,
        "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 0})
}

/// A checkpoint that never finishes, as a daemon that never answers would.
#[derive(Debug)]
struct Hanging;

#[async_trait::async_trait]
impl Checkpoint for Hanging {
    async fn push(&self, _memory: Vec<MemoryDelta<'static>>) -> crate::Result<()> {
        std::future::pending().await
    }
}

/// A checkpoint the daemon refuses every time.
#[derive(Debug)]
struct Refused;

#[async_trait::async_trait]
impl Checkpoint for Refused {
    async fn push(&self, _memory: Vec<MemoryDelta<'static>>) -> crate::Result<()> {
        Err(crate::Error::checkpoint(afr_providers::Error::refused(503)))
    }
}

/// The key of the one note the checkpoint tests store.
const HELD: &str = "held";

/// A model call storing the note under [`HELD`].
fn store_held() -> Chunk {
    call(
        "store",
        MEMORY_STORE.name(),
        json!({"key": HELD, "content": "x"}),
    )
}

/// Runs a lease that stores one entry and then answers, checkpointing after
/// every call into `checkpoint`.
async fn store_then_answer(checkpoint: &dyn Checkpoint) -> RunOutput {
    let script = Script::new([vec![store_held()], vec![say("answered")]]);
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let lease = lease(&[MEMORY_STORE.name()], every_call());
    let frames = Frames::default();
    let sink = frames.sink();

    let output = engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &CountingMint::never(),
            checkpoint,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();
    frames.taken();
    output
}

#[tokio::test]
async fn a_refused_checkpoint_is_logged_under_its_code_and_the_run_goes_on() {
    let capture = Capture::install();

    let output = store_then_answer(&Refused).await;

    assert_eq!(output.result.content, "answered");
    let failed = capture.only(EVENT_CHECKPOINT_FAILED);
    assert_eq!(failed.level, tracing::Level::WARN);
    let code = afr_providers::Error::refused(503).code();
    assert_eq!(failed.field("error_code"), Some(code.as_str()));
    assert_eq!(failed.field("lease_id"), Some("lease-1"));
}

#[tokio::test]
async fn a_discarded_checkpoint_fails_nothing_and_leaves_the_final_push_whole() {
    let capture = Capture::install();

    let output = store_then_answer(&Discard).await;

    assert_eq!(output.result.content, "answered");
    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.field("event") != Some(EVENT_CHECKPOINT_FAILED)),
        "a discarded checkpoint is not a failed one"
    );
    let pushed: Vec<_> = output
        .memory
        .iter()
        .map(|delta| delta.key.as_ref())
        .collect();
    assert_eq!(pushed, [HELD], "the final push still carries it");
}

#[tokio::test(start_paused = true)]
async fn a_checkpoint_that_never_answers_does_not_outlive_the_lease() {
    let script = Script::new([vec![store_held()], vec![say("never reached")]]);
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let lease = lease(&[MEMORY_STORE.name()], every_call());
    let frames = Frames::default();
    let sink = frames.sink();
    let stop = CancellationToken::new();
    let stopper = stop.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        stopper.cancel();
    });

    let ran = tokio::time::timeout(
        Duration::from_secs(60),
        engine.run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &CountingMint::never(),
            checkpoint: &Hanging,
            events: &sink,
            meter: &Meter::default(),
            stop: &stop,
        }),
    )
    .await;
    frames.taken();

    ran.unwrap().unwrap();
}

#[tokio::test]
async fn a_cadence_with_nothing_stored_pushes_nothing() {
    let list = || vec![call("l", MEMORY_LIST.name(), json!({}))];
    let script = Script::new([list(), list(), vec![say("nothing stored")]]);
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let lease = lease(&[MEMORY_LIST.name()], every_call());
    let (checkpoint, pushed) = Recording::new();
    let frames = Frames::default();
    let sink = frames.sink();

    engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &CountingMint::never(),
            checkpoint: &checkpoint,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();
    frames.taken();

    assert_eq!(pushed.try_iter().count(), 0);
}
