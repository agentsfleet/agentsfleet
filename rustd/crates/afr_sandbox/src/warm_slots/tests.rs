#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake it cannot build"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use afr_executor::{DirEntry, Executor, FileContent, Process, ProcessId, Spawn};
use bytes::Bytes;

use super::WarmSlots;
use crate::engine::{Engine, Limits, Sandbox, SandboxRequest};
use crate::error::Result;

/// An executor nothing calls; a warm slot is only handed out, never driven.
#[derive(Debug)]
struct Idle;

fn unused<T>() -> afr_executor::Result<T> {
    Err(std::io::Error::other("an idle fake").into())
}

#[async_trait::async_trait]
impl Executor for Idle {
    async fn spawn(&self, _spawn: Spawn) -> afr_executor::Result<Process> {
        unused()
    }
    async fn write(&self, _process: ProcessId, _data: Bytes) -> afr_executor::Result<()> {
        unused()
    }
    async fn kill(&self, _process: ProcessId) -> afr_executor::Result<()> {
        unused()
    }
    async fn read_file(&self, _path: &str, _max_bytes: u64) -> afr_executor::Result<FileContent> {
        unused()
    }
    async fn write_file(&self, _path: &str, _data: Bytes) -> afr_executor::Result<()> {
        unused()
    }
    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Vec<DirEntry>> {
        unused()
    }
}

/// A sandbox that remembers its name and counts its own destruction.
#[derive(Debug)]
struct Named {
    panics: bool,
    #[expect(
        dead_code,
        reason = "read through the derived Debug rendering the tests inspect"
    )]
    name: String,
    destroyed: Arc<AtomicU64>,
    executor: Idle,
}

#[async_trait::async_trait]
impl Sandbox for Named {
    fn executor(&self) -> &dyn Executor {
        &self.executor
    }
    async fn destroy(self: Box<Self>) -> Result<()> {
        assert!(!self.panics, "a teardown that panics");
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// An engine that names each sandbox after its request and counts them.
#[derive(Debug, Default)]
struct Counting {
    started: AtomicU64,
    destroyed: Arc<AtomicU64>,
    refuse: bool,
    /// How long each start takes once counted.
    delay: Duration,
    /// Whether each sandbox it starts fails loudly when destroyed.
    panics: bool,
}

#[async_trait::async_trait]
impl Engine for Counting {
    async fn prepare(&self, request: SandboxRequest<'_>) -> Result<Box<dyn Sandbox>> {
        if self.refuse {
            return Err(crate::error::refused("landlock"));
        }
        self.started.fetch_add(1, Ordering::SeqCst);
        // A zero sleep still waits for the timer, which the yields in
        // `settle` never turn; only a delayed start sleeps at all.
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(Box::new(Named {
            name: request.lease_id.to_owned(),
            destroyed: Arc::clone(&self.destroyed),
            panics: self.panics,
            executor: Idle,
        }))
    }
}

/// What a sandbox handed out was named, read through its debug rendering.
fn name_of(sandbox: &dyn Sandbox) -> String {
    format!("{sandbox:?}")
}

async fn settle() {
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn test_warm_slot_single_use() {
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    let request = SandboxRequest {
        lease_id: "lease-a",
        limits: Limits::default(),
    };

    let first = slots.prepare(request).await.unwrap();
    settle().await;
    let second = slots
        .prepare(SandboxRequest {
            lease_id: "lease-b",
            ..request
        })
        .await
        .unwrap();

    assert!(name_of(first.as_ref()).contains("warm-1"), "{first:?}");
    assert!(
        name_of(second.as_ref()).contains("warm-2"),
        "a replacement, never the first again"
    );
    first.destroy().await.unwrap();
    second.destroy().await.unwrap();
    settle().await;
    assert_eq!(
        inner.started.load(Ordering::SeqCst),
        3,
        "two handed out, one waiting"
    );
    slots.shutdown().await;
    assert_eq!(
        inner.destroyed.load(Ordering::SeqCst),
        3,
        "the waiting slot is retired too"
    );
}

#[tokio::test]
async fn test_a_request_with_other_limits_starts_cold() {
    let inner = Arc::new(Counting::default());
    let capture = afd_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    let cold = slots
        .prepare(SandboxRequest {
            lease_id: "lease-c",
            limits: Limits {
                pids: 7,
                ..Limits::default()
            },
        })
        .await
        .unwrap();

    assert!(name_of(cold.as_ref()).contains("lease-c"));
    assert_eq!(
        capture.only("sandbox_start_ms").field("start"),
        Some("cold")
    );
    cold.destroy().await.unwrap();
    slots.shutdown().await;
}

#[tokio::test]
async fn test_zero_slots_makes_every_start_cold() {
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 0, Limits::default());

    let cold = slots
        .prepare(SandboxRequest {
            lease_id: "lease-d",
            limits: Limits::default(),
        })
        .await
        .unwrap();

    assert!(name_of(cold.as_ref()).contains("lease-d"));
    cold.destroy().await.unwrap();
    slots.shutdown().await;
}

#[tokio::test]
async fn test_a_slot_that_fails_to_start_is_logged_and_the_lease_starts_cold() {
    let inner = Arc::new(Counting {
        refuse: true,
        ..Counting::default()
    });
    let capture = afd_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(inner, 1, Limits::default());
    settle().await;

    let refused = slots
        .prepare(SandboxRequest {
            lease_id: "lease-e",
            limits: Limits::default(),
        })
        .await;

    assert!(refused.is_err(), "the cold start is refused the same way");
    assert_eq!(
        capture.only("sandbox_warm_slot_failed").field("lease_id"),
        Some("warm-1")
    );
    slots.shutdown().await;
}

/// A slot still starting when the keeper shuts down is waited for and torn
/// down, never abandoned with its cgroup and disk in place.
#[tokio::test(start_paused = true)]
async fn test_shutdown_waits_for_a_start_still_in_flight() {
    let inner = Arc::new(Counting {
        delay: Duration::from_secs(1),
        ..Counting::default()
    });
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    assert_eq!(
        inner.started.load(Ordering::SeqCst),
        1,
        "the slot is mid-start"
    );

    slots.shutdown().await;

    assert_eq!(inner.destroyed.load(Ordering::SeqCst), 1);
}

/// A lease that stops waiting after its claim is answered never strands the
/// slot it was handed: the keeper takes it back and tears it down.
#[tokio::test]
async fn test_a_slot_whose_lease_stopped_waiting_is_retired() {
    use futures_util::FutureExt as _;
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    let abandoned = slots
        .prepare(SandboxRequest {
            lease_id: "lease-f",
            limits: Limits::default(),
        })
        .now_or_never();
    settle().await;

    assert!(
        abandoned.is_none(),
        "the claim was still waiting when dropped"
    );
    assert_eq!(inner.destroyed.load(Ordering::SeqCst), 1);
    slots.shutdown().await;
}

/// A keeper that dies is reported by shutdown rather than ignored.
#[tokio::test]
async fn test_a_keeper_that_dies_is_reported_at_shutdown() {
    let inner = Arc::new(Counting {
        panics: true,
        ..Counting::default()
    });
    let capture = afd_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    slots.shutdown().await;

    assert!(
        capture
            .only("sandbox_warm_slot_left")
            .field("reason")
            .is_some_and(|reason| reason.contains("panicked"))
    );
}
