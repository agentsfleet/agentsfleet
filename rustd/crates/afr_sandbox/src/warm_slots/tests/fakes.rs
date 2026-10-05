//! The engine and sandbox the warm-slot tests count with.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use afr_executor::{Executor, FileContent, Listing, Process, ProcessId, Spawn};
use bytes::Bytes;

use crate::engine::{Engine, Sandbox, SandboxRequest};
use crate::error::Result;

/// An executor nothing calls; a warm slot is only handed out, never driven.
#[derive(Debug)]
pub(super) struct Idle;

fn unused<T>() -> afr_executor::Result<T> {
    Err(std::io::Error::other("an idle fake").into())
}

#[async_trait::async_trait]
impl Executor for Idle {
    async fn spawn(&self, _spawn: &Spawn) -> afr_executor::Result<Process> {
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
    async fn append_file(&self, _path: &str, _data: Bytes) -> afr_executor::Result<()> {
        unused()
    }
    async fn delete_file(&self, _path: &str) -> afr_executor::Result<()> {
        unused()
    }
    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Listing> {
        unused()
    }
}

/// A sandbox that remembers its name and counts its own destruction.
#[derive(Debug)]
pub(super) struct Named {
    pub(super) panics: bool,
    pub(super) dead: bool,
    #[expect(
        dead_code,
        reason = "read through the derived Debug rendering the tests inspect"
    )]
    name: String,
    pub(super) destroyed: Arc<AtomicU64>,
    executor: Idle,
}

#[async_trait::async_trait]
impl Sandbox for Named {
    fn executor(&self) -> &dyn Executor {
        &self.executor
    }
    fn is_running(&mut self) -> bool {
        !self.dead
    }
    async fn destroy(self: Box<Self>) -> Result<()> {
        assert!(!self.panics, "a teardown that panics");
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// An engine that names each sandbox after its request and counts them.
#[derive(Debug, Default)]
pub(super) struct Counting {
    pub(super) started: AtomicU64,
    pub(super) destroyed: Arc<AtomicU64>,
    /// How many starts to refuse before one succeeds.
    pub(super) refusals: AtomicU64,
    /// Whether each sandbox it starts has already died.
    pub(super) dead: bool,
    /// How long each start takes once counted.
    pub(super) delay: Duration,
    /// Whether each sandbox it starts fails loudly when destroyed.
    pub(super) panics: bool,
}

#[async_trait::async_trait]
impl Engine for Counting {
    async fn prepare(&self, request: SandboxRequest<'_>) -> Result<Box<dyn Sandbox>> {
        let refused = self
            .refusals
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            });
        if refused.is_ok() {
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
            dead: self.dead,
            executor: Idle,
        }))
    }
}

/// What a sandbox handed out was named, read through its debug rendering.
pub(super) fn name_of(sandbox: &dyn Sandbox) -> String {
    format!("{sandbox:?}")
}

pub(super) async fn settle() {
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}
