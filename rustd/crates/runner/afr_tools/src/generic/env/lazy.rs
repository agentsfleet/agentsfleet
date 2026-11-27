//! The run's sandbox for the tools a run built before the sandbox was ready.

use std::time::Duration;

use af_guest_wire::{Sizes, WORKSPACE_ROOT};
use afr_guest_client::AgentSource;
use bytes::Bytes;
use guffbolt::tools::env::{
    Browser, Env, EnvError, EnvErrorKind, EnvFuture, FileContent, Found, Listing, Process,
    ProcessId, Search, Spawn,
};

use super::{ENDING_GRACE, SandboxEnv, WAITS_FOR_FIRECRACKER};

/// The run's sandbox, as the environment of the tools the run built before
/// the sandbox was ready: every call gets the guest agent from `source` first.
#[derive(Debug, Clone, Copy)]
pub struct LazySandboxEnv<'a> {
    source: &'a dyn AgentSource,
    /// The sizes the run's guest agent is started with, known before it is:
    /// a tool asks for its read size before the call that waits for the
    /// sandbox.
    sizes: Sizes,
}

impl<'a> LazySandboxEnv<'a> {
    /// The environment of the sandbox `source` hands over, once it does,
    /// whose guest agent holds to `sizes`.
    #[must_use]
    pub const fn new(source: &'a dyn AgentSource, sizes: Sizes) -> Self {
        Self { source, sizes }
    }

    /// The sandbox's environment, waiting for the build when it has not
    /// finished; why there is none, in the words a sandbox-side call reads,
    /// when the run has none.
    async fn sandbox(&self) -> Result<SandboxEnv<'a>, EnvError> {
        self.source
            .get()
            .await
            .map(SandboxEnv::new)
            .map_err(|why| EnvError::new(EnvErrorKind::Unavailable, why.reason()))
    }
}

impl Env for LazySandboxEnv<'_> {
    fn root(&self) -> &str {
        WORKSPACE_ROOT
    }

    fn spawn<'a>(&'a self, spawn: &'a Spawn) -> EnvFuture<'a, Process> {
        Box::pin(async move { self.sandbox().await?.spawn(spawn).await })
    }

    fn write(&self, process: ProcessId, data: Bytes) -> EnvFuture<'_, ()> {
        Box::pin(async move { self.sandbox().await?.write(process, data).await })
    }

    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()> {
        Box::pin(async move { self.sandbox().await?.kill(process).await })
    }

    fn read_file<'a>(&'a self, path: &'a str, max_bytes: u64) -> EnvFuture<'a, FileContent> {
        Box::pin(async move { self.sandbox().await?.read_file(path, max_bytes).await })
    }

    fn write_file<'a>(&'a self, path: &'a str, data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(async move { self.sandbox().await?.write_file(path, data).await })
    }

    fn delete_file<'a>(&'a self, path: &'a str) -> EnvFuture<'a, ()> {
        Box::pin(async move { self.sandbox().await?.delete_file(path).await })
    }

    fn list_dir<'a>(&'a self, path: &'a str) -> EnvFuture<'a, Listing> {
        Box::pin(async move { self.sandbox().await?.list_dir(path).await })
    }

    fn search<'a>(&'a self, search: &'a Search) -> EnvFuture<'a, Found> {
        Box::pin(async move { self.sandbox().await?.search(search).await })
    }

    fn ending_grace(&self) -> Duration {
        ENDING_GRACE
    }

    fn max_read_bytes(&self) -> u64 {
        self.sizes.max_read_bytes
    }

    fn edge_bytes(&self) -> usize {
        self.sizes.edge_bytes
    }

    fn browser(&self) -> Result<&dyn Browser, &str> {
        Err(WAITS_FOR_FIRECRACKER)
    }
}
