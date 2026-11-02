//! A chat's first words reach the fleet's live tail while its sandbox is still
//! being built: through the daemon that ships, the runner's build runs beside
//! the turn's first model call, and the turn's tool call still runs in the
//! sandbox once it is ready.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use afd_state::sql::LEASE_STATUS_REPORTED;
use afr_sandbox::{Engine, Sandbox, SandboxRequest, UnsandboxedEngine};
use agentsfleetd::supervisor::Supervisor;
use tokio::time::Instant;

use super::{ANSWER, SETTLE_DEADLINE, TOOL, allow_all_egress, rust_runner, settled_status};
use crate::e2e::scenario;

/// How long the stalled engine holds every prepare: long enough that a turn
/// waiting for its sandbox could not stream first by accident.
const PREPARE_TAKES: Duration = Duration::from_secs(2);
/// The frame kind the turn's answer streams as.
const CHUNK: &str = "chunk";

/// The unsandboxed engine, answering each prepare only after
/// [`PREPARE_TAKES`] and noting when the first one was ready.
#[derive(Debug)]
struct Stalled {
    inner: UnsandboxedEngine,
    ready: Arc<Mutex<Option<Instant>>>,
}

#[async_trait::async_trait]
impl Engine for Stalled {
    async fn prepare(&self, request: SandboxRequest<'_>) -> afr_sandbox::Result<Box<dyn Sandbox>> {
        tokio::time::sleep(PREPARE_TAKES).await;
        let prepared = self.inner.prepare(request).await;
        self.ready
            .lock()
            .expect("nothing panics holding the clock")
            .get_or_insert_with(Instant::now);
        prepared
    }
}

/// A turn that answers first, then runs one process in its sandbox.
fn answering_first() -> afr_agent::scripted::ScriptedEngine {
    use afr_agent::scripted::{ScriptedEngine, Step};
    ScriptedEngine::new([
        Step::Say(ANSWER.to_owned()),
        Step::Run {
            tool: TOOL,
            spawn: afr_executor::Spawn::program("echo").arg("hi"),
        },
    ])
}

/// When the first answer chunk reached the tail, read while the runner runs.
async fn chunk_arrived(tail: &mut afd_dragonfly::Subscription) -> Option<Instant> {
    let deadline = Instant::now() + SETTLE_DEADLINE;
    while Instant::now() < deadline {
        let Some(frame) = crate::tail::next_frame(tail).await else {
            continue;
        };
        if frame.get("kind").and_then(serde_json::Value::as_str) == Some(CHUNK) {
            return Some(Instant::now());
        }
    }
    None
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_chat_streams_before_sandbox_ready() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    allow_all_egress(&run).await;
    let hub = afd_dragonfly::SubscriptionHub::start(crate::e2e::dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", run.fleet));
    crate::tail::settle().await;
    let home = tempfile::tempdir().expect("a storage home");
    let sandboxes = tempfile::tempdir_in("/tmp").expect("a short sandbox base");
    let shutdown = tokio_util::sync::CancellationToken::new();
    let ready = Arc::default();
    let engine = Stalled {
        inner: UnsandboxedEngine::new(sandboxes.path().to_owned())
            .expect("a debug build permits the unsandboxed engine"),
        ready: Arc::clone(&ready),
    };
    let runner = tokio::spawn(rust_runner(
        &run,
        home.path(),
        (Box::new(engine), Box::new(answering_first())),
        shutdown.clone(),
    ));

    let streamed = chunk_arrived(&mut tail).await.expect("the answer streamed");
    assert_eq!(
        settled_status(&run).await.as_deref(),
        Some(LEASE_STATUS_REPORTED)
    );
    let ready = ready
        .lock()
        .expect("nothing panics holding the clock")
        .expect("the turn's tool call built a sandbox");
    assert!(
        streamed < ready,
        "the first chunk streamed {:?} before the sandbox was ready",
        ready.saturating_duration_since(streamed)
    );
    let status = crate::reads::event_column(&run, &run.event_id, "status").await;
    assert_eq!(status.as_deref(), Some("processed"), "the tool ran in it");

    shutdown.cancel();
    runner
        .await
        .expect("the runner task joins")
        .expect("the runner stops cleanly on shutdown");
    drop(tail);
    supervisor.shutdown().await;
    run.cleanup().await;
}
