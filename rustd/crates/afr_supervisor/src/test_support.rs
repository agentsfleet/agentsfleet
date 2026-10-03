//! Fakes for the supervisor's seams: the daemon, the engine, its sandbox and
//! executor, and the agent. Every one is driven by the suites that use it.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assertions_on_result_states,
    clippy::panic,
    reason = "test support: a fixture that cannot be built is a broken test"
)]

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_wire::activity::{ActivityFrame, FleetResponseChunk};
use afd_wire::lease::{BundleManifest, LeasePayload};
use afd_wire::memory::MemoryDelta;
use afd_wire::report::{Completed, ExecutionResult, ResultOutcome};
use afr_agent::{AgentEngine, AgentRun, RunOutput};
use afr_executor::{Executor, ProcessId, Spawn};
use bytes::Bytes;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::client::{Call, ControlPlane, RunnerApi};

mod rig;
#[path = "test_support/sandbox.rs"]
mod sandbox;

pub(crate) use self::rig::{Rig, daemon, position, reported};
pub(crate) use self::sandbox::{FakeEngine, Writes};

/// A canonical lease identifier.
pub(crate) const LEASE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8057";
/// A canonical fleet identifier.
pub(crate) const FLEET_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8058";
/// The fencing token every fake lease carries.
pub(crate) const FENCING: u64 = 504;
/// When every fake lease is granted until, in Unix milliseconds: thirty
/// seconds after the fixed clock's zero.
pub(crate) const GRANTED_UNTIL: i64 = 30_000;

/// The cadence and retry delay the fake daemon answers with, in milliseconds.
pub(crate) const INTERVAL_MS: u32 = 1000;

/// The report fields, and the spellings, the lease tests read back.
pub(crate) const FAILURE_REASON: &str = "failure_reason";
pub(crate) const OUTCOME: &str = "outcome";
pub(crate) const PROCESSED: &str = "processed";
pub(crate) const RENEWAL_TERMINATE: &str = "renewal_terminate";
pub(crate) const STARTUP_POSTURE: &str = "startup_posture";
pub(crate) const RUNNER_CRASH: &str = "runner_crash";

/// The wall clock every lease test reads: fixed at zero, so a lease's deadline
/// is measured in the paused tokio time the tests advance.
pub(crate) fn clock() -> Box<dyn afd_core::clock::Clock> {
    Box::new(afd_core::clock::FixedClock::at(
        afd_core::clock::UnixMillis::from_millis(0),
    ))
}

/// What the fake daemon does with one call.
pub(crate) enum Answer {
    /// Replies 2xx with these bytes.
    Reply(Bytes),
    /// Fails with this error.
    Fail(crate::Error),
    /// Never answers.
    Stall,
    /// Replies 2xx with these bytes once this long has passed.
    Late(Duration, Bytes),
}

/// A reply of `value`, encoded.
pub(crate) fn json<T: Serialize>(value: &T) -> Answer {
    Answer::Reply(Bytes::from(serde_json::to_vec(value).unwrap()))
}

/// The daemon, answering every call from one closure and recording each.
pub(crate) struct FakeApi<F> {
    answer: F,
    calls: mpsc::UnboundedSender<Call>,
}

impl<F> fmt::Debug for FakeApi<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FakeApi")
    }
}

#[async_trait::async_trait]
impl<F> RunnerApi for FakeApi<F>
where
    F: Fn(&Call) -> Answer + Send + Sync,
{
    async fn send(&self, call: Call) -> crate::Result<Bytes> {
        let answer = (self.answer)(&call);
        drop(self.calls.send(call));
        match answer {
            Answer::Reply(bytes) => Ok(bytes),
            Answer::Fail(failure) => Err(failure),
            Answer::Stall => std::future::pending().await,
            Answer::Late(after, bytes) => {
                tokio::time::sleep(after).await;
                Ok(bytes)
            }
        }
    }
}

/// A control plane over a fake daemon, and the calls it receives.
pub(crate) fn plane<F>(answer: F) -> (ControlPlane, mpsc::UnboundedReceiver<Call>)
where
    F: Fn(&Call) -> Answer + Send + Sync + 'static,
{
    let (calls, received) = mpsc::unbounded_channel();
    (
        ControlPlane::new(Box::new(FakeApi { answer, calls })),
        received,
    )
}

/// Every call received so far.
pub(crate) fn drain(received: &mut mpsc::UnboundedReceiver<Call>) -> Vec<Call> {
    std::iter::from_fn(|| received.try_recv().ok()).collect()
}

/// A lease for `fleet`, with or without a bundle.
pub(crate) fn lease(lease_id: &str, fleet: &str, bundle: Option<&str>) -> LeasePayload<'static> {
    let mut document: serde_json::Value = serde_json::from_str(LEASE_JSON).unwrap();
    document["lease_id"] = lease_id.into();
    document["event"]["fleet_id"] = fleet.into();
    // Leaked so the borrowed payload lives as long as the test that reads it.
    let text: &'static str = Box::leak(document.to_string().into_boxed_str());
    let mut payload: LeasePayload<'static> = serde_json::from_str(text).unwrap();
    payload.bundle = bundle.map(|hash| BundleManifest {
        content_hash: hash.to_owned().into(),
    });
    payload
}

const LEASE_JSON: &str = include_str!("test_support/lease.json");

/// What the fake agent does with a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Behaviour {
    /// Emits one chunk, drives the executor, and answers with one memory delta.
    Answer,
    /// Fails as an engine.
    Break,
    /// Never finishes on its own.
    Hang,
    /// Panics mid-run.
    Panic,
}

/// An agent engine that counts its runs and how many overlap.
#[derive(Debug)]
pub(crate) struct FakeAgent {
    pub(crate) behaviour: Behaviour,
    pub(crate) runs: Arc<AtomicUsize>,
    pub(crate) peak: Arc<AtomicUsize>,
    running: AtomicUsize,
}

impl FakeAgent {
    pub(crate) fn new(behaviour: Behaviour) -> Self {
        Self {
            behaviour,
            runs: Arc::default(),
            peak: Arc::default(),
            running: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl AgentEngine for FakeAgent {
    async fn run(&self, run: AgentRun<'_>) -> afr_agent::Result<RunOutput> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let overlapping = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(overlapping, Ordering::SeqCst);
        run.events
            .emit(ActivityFrame::FleetResponseChunk(FleetResponseChunk {
                text: "hi".into(),
                text_kind: None,
                first_chunk_after_ms: None,
                stream_start: true,
                stream_contiguous: false,
                stream_seq: 0,
            }));
        exercise(run.executor.unwrap()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.running.fetch_sub(1, Ordering::SeqCst);
        match self.behaviour {
            Behaviour::Answer => Ok(answer()),
            Behaviour::Break => {
                Err(afr_executor::Error::from(std::io::Error::other("engine broke")).into())
            }
            Behaviour::Hang => std::future::pending().await,
            Behaviour::Panic => panic!("the fake engine panics on purpose"),
        }
    }
}

/// Calls every executor method, as a turn's tools would.
async fn exercise(executor: &dyn Executor) {
    let id = ProcessId::new(1);
    assert!(executor.spawn(&Spawn::program("true")).await.is_err());
    assert!(executor.write(id, Bytes::new()).await.is_ok());
    assert!(executor.kill(id).await.is_ok());
    assert!(executor.read_file("a", 1).await.is_ok());
    assert!(executor.write_file("a", Bytes::new()).await.is_ok());
    assert!(executor.list_dir("/").await.unwrap().entries.is_empty());
}

/// The result a successful fake run answers with.
pub(crate) fn answer() -> RunOutput {
    RunOutput {
        result: ExecutionResult {
            outcome: ResultOutcome::Completed(Completed {}),
            content: "done".into(),
            token_count: 7,
            wall_seconds: 1,
            memory_peak_bytes: 0,
            cpu_throttled_ms: 0,
            input_tokens: 3,
            cached_input_tokens: 1,
            output_tokens: 4,
        },
        memory: vec![MemoryDelta {
            key: "k".into(),
            content: "v".into(),
            category: "core".into(),
        }],
    }
}
