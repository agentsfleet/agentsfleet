//! A sandbox engine for the lease tests: sandboxes that count their teardowns
//! and an executor that reports what is written into the workspace.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afr_executor::{Executor, FileContent, Listing, Process, ProcessId, Spawn};
use afr_sandbox::{Engine, Sandbox, SandboxRequest};
use bytes::Bytes;
use tokio::sync::mpsc;

/// An engine whose sandboxes count their teardowns.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag injects one independent fault into the sandbox a test is given"
)]
#[derive(Debug, Default)]
pub(crate) struct FakeEngine {
    pub(crate) refuse: bool,
    /// Panics on the first prepare only, the way a bug in the supervisor
    /// would take its worker down.
    pub(crate) panic_once: bool,
    pub(crate) fail_teardown: bool,
    pub(crate) prepared: Arc<AtomicUsize>,
    pub(crate) destroyed: Arc<AtomicUsize>,
    /// Where each sandbox's executor reports the files written into it.
    pub(crate) written: Option<mpsc::UnboundedSender<(String, Bytes)>>,
    /// Whether those executors refuse every file write.
    pub(crate) refuse_writes: bool,
}

#[async_trait::async_trait]
impl Engine for FakeEngine {
    async fn prepare(&self, _request: SandboxRequest<'_>) -> afr_sandbox::Result<Box<dyn Sandbox>> {
        if self.refuse {
            return Err(std::io::Error::other("no landlock").into());
        }
        let prepared = self.prepared.fetch_add(1, Ordering::SeqCst);
        assert!(
            !(self.panic_once && prepared == 0),
            "the fake engine panics on its first prepare"
        );
        Ok(Box::new(FakeSandbox {
            fail_teardown: self.fail_teardown,
            destroyed: Arc::clone(&self.destroyed),
            executor: FakeExecutor {
                written: self.written.clone(),
                refuse_writes: self.refuse_writes,
            },
        }))
    }
}

#[derive(Debug)]
struct FakeSandbox {
    fail_teardown: bool,
    destroyed: Arc<AtomicUsize>,
    executor: FakeExecutor,
}

#[async_trait::async_trait]
impl Sandbox for FakeSandbox {
    fn executor(&self) -> &dyn Executor {
        &self.executor
    }

    async fn destroy(self: Box<Self>) -> afr_sandbox::Result<()> {
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        if self.fail_teardown {
            return Err(std::io::Error::other("busy mount").into());
        }
        Ok(())
    }
}

/// An executor that refuses to spawn, reports the files written into it, and
/// answers everything else emptily.
#[derive(Debug)]
struct FakeExecutor {
    written: Option<mpsc::UnboundedSender<(String, Bytes)>>,
    refuse_writes: bool,
}

#[async_trait::async_trait]
impl Executor for FakeExecutor {
    async fn spawn(&self, _spawn: Spawn) -> afr_executor::Result<Process> {
        Err(std::io::Error::other("no processes here").into())
    }

    async fn write(&self, _process: ProcessId, _data: Bytes) -> afr_executor::Result<()> {
        Ok(())
    }

    async fn kill(&self, _process: ProcessId) -> afr_executor::Result<()> {
        Ok(())
    }

    async fn read_file(&self, _path: &str, _max_bytes: u64) -> afr_executor::Result<FileContent> {
        Ok(FileContent {
            data: Bytes::new(),
            truncated: false,
        })
    }

    async fn write_file(&self, path: &str, data: Bytes) -> afr_executor::Result<()> {
        if self.refuse_writes {
            return Err(std::io::Error::other("read-only workspace").into());
        }
        if let Some(written) = &self.written {
            let _reader_gone = written.send((path.to_owned(), data));
        }
        Ok(())
    }

    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Listing> {
        Ok(Listing {
            entries: Vec::new(),
            truncated: false,
        })
    }
}
