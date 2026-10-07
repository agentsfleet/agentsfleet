//! A sandbox engine for the lease tests: sandboxes that count their teardowns
//! and an executor that reports what is written into the workspace.

use std::os::unix::fs::MetadataExt as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use afr_executor::{Executor, FileContent, Listing, Process, ProcessId, Spawn};
use afr_sandbox::{Engine, HostWorkspace, Limits, Sandbox, SandboxRequest};
use bytes::Bytes;
use tokio::sync::mpsc;

/// How a fake executor answers a file write.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Writes {
    /// Takes it.
    #[default]
    Accept,
    /// Refuses it, as a read-only workspace would.
    Refuse,
    /// Never answers, as a stopped executor would.
    Stall,
}

/// What a fake sandbox does when it is frozen and when it is thawed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Freezer {
    /// Freezes and thaws.
    #[default]
    Works,
    /// Refuses the freeze, as a host without the cgroup freezer would.
    RefusesFreeze,
    /// Freezes, then refuses the thaw.
    RefusesThaw,
    /// Freezes and thaws, but its executor never answers again, as one that
    /// died while frozen would.
    ThawsSilent,
    /// Freezes and thaws, but its executor fails every listing after.
    ThawsBroken,
}

/// An engine whose sandboxes count their teardowns.
#[derive(Debug, Default)]
pub(crate) struct FakeEngine {
    pub(crate) refuse: bool,
    /// Panics on the first prepare only, the way a bug in the supervisor
    /// would take its worker down.
    pub(crate) panic_once: bool,
    pub(crate) fail_teardown: bool,
    /// How long each teardown takes before it is counted.
    pub(crate) teardown_takes: Duration,
    pub(crate) prepared: Arc<AtomicUsize>,
    pub(crate) destroyed: Arc<AtomicUsize>,
    /// Where each sandbox's executor reports the files written into it.
    pub(crate) written: Option<mpsc::UnboundedSender<(String, Bytes)>>,
    /// How those executors answer a file write.
    pub(crate) writes: Writes,
    /// The host directory each sandbox offers as its workspace, owned by
    /// whoever owns it; none keeps the workspace out of the host's reach.
    pub(crate) workspace: Option<PathBuf>,
    /// Where each prepare reports the limits it was asked to enforce.
    pub(crate) asked: Option<mpsc::UnboundedSender<Limits>>,
    /// How many times its sandboxes were frozen, and thawed.
    pub(crate) frozen: Arc<AtomicUsize>,
    pub(crate) thawed: Arc<AtomicUsize>,
    /// What its sandboxes do when frozen and thawed.
    pub(crate) freezer: Freezer,
}

#[async_trait::async_trait]
impl Engine for FakeEngine {
    async fn prepare(&self, request: SandboxRequest<'_>) -> afr_sandbox::Result<Box<dyn Sandbox>> {
        if let Some(asked) = &self.asked {
            let _ = asked.send(request.limits);
        }
        if self.refuse {
            return Err(std::io::Error::other("no landlock").into());
        }
        let prepared = self.prepared.fetch_add(1, Ordering::SeqCst);
        assert!(
            !(self.panic_once && prepared == 0),
            "the fake engine panics on its first prepare"
        );
        let workspace = self.workspace.clone().map(|root| {
            let metadata = std::fs::metadata(&root).unwrap();
            (root, (metadata.uid(), metadata.gid()))
        });
        Ok(Box::new(FakeSandbox {
            fail_teardown: self.fail_teardown,
            teardown_takes: self.teardown_takes,
            workspace,
            destroyed: Arc::clone(&self.destroyed),
            frozen: Arc::clone(&self.frozen),
            thawed: Arc::clone(&self.thawed),
            freezer: self.freezer,
            executor: FakeExecutor {
                written: self.written.clone(),
                writes: self.writes,
                freezer: self.freezer,
                thawed: AtomicBool::default(),
            },
        }))
    }
}

#[derive(Debug)]
struct FakeSandbox {
    fail_teardown: bool,
    teardown_takes: Duration,
    workspace: Option<(PathBuf, (u32, u32))>,
    destroyed: Arc<AtomicUsize>,
    frozen: Arc<AtomicUsize>,
    thawed: Arc<AtomicUsize>,
    freezer: Freezer,
    executor: FakeExecutor,
}

#[async_trait::async_trait]
impl Sandbox for FakeSandbox {
    fn executor(&self) -> &dyn Executor {
        &self.executor
    }

    fn workspace(&self) -> Option<HostWorkspace<'_>> {
        self.workspace.as_ref().map(|(root, owner)| HostWorkspace {
            root,
            owner: *owner,
        })
    }

    async fn freeze(&self) -> afr_sandbox::Result<()> {
        if self.freezer == Freezer::RefusesFreeze {
            return Err(std::io::Error::other(NO_FREEZER).into());
        }
        self.frozen.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn thaw(&self) -> afr_sandbox::Result<()> {
        if self.freezer == Freezer::RefusesThaw {
            return Err(std::io::Error::other(NO_FREEZER).into());
        }
        self.thawed.fetch_add(1, Ordering::SeqCst);
        self.executor.thawed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn destroy(self: Box<Self>) -> afr_sandbox::Result<()> {
        if !self.teardown_takes.is_zero() {
            tokio::time::sleep(self.teardown_takes).await;
        }
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        if self.fail_teardown {
            return Err(std::io::Error::other("busy mount").into());
        }
        Ok(())
    }
}

/// What a refused write or delete says.
const READ_ONLY: &str = "read-only workspace";
/// What a refused freeze or thaw says.
const NO_FREEZER: &str = "cgroup.freeze refused";
/// What a broken executor says once its sandbox is thawed.
pub(crate) const EXECUTOR_GONE: &str = "the executor died while frozen";

/// An executor that refuses to spawn, reports the files written into it, and
/// answers everything else emptily, until its sandbox's freezer says not.
#[derive(Debug, Default)]
struct FakeExecutor {
    written: Option<mpsc::UnboundedSender<(String, Bytes)>>,
    writes: Writes,
    /// What its sandbox does when thawed.
    freezer: Freezer,
    /// Whether its sandbox was thawed.
    thawed: AtomicBool,
}

#[async_trait::async_trait]
impl Executor for FakeExecutor {
    async fn spawn(&self, _spawn: &Spawn) -> afr_executor::Result<Process> {
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
        match self.writes {
            Writes::Accept => {}
            Writes::Refuse => return Err(std::io::Error::other(READ_ONLY).into()),
            Writes::Stall => std::future::pending::<()>().await,
        }
        if let Some(written) = &self.written {
            let _reader_gone = written.send((path.to_owned(), data));
        }
        Ok(())
    }

    async fn append_file(&self, path: &str, data: Bytes) -> afr_executor::Result<()> {
        self.write_file(path, data).await
    }

    async fn delete_file(&self, _path: &str) -> afr_executor::Result<()> {
        match self.writes {
            Writes::Accept => Ok(()),
            Writes::Refuse => Err(std::io::Error::other(READ_ONLY).into()),
            Writes::Stall => std::future::pending().await,
        }
    }

    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Listing> {
        let thawed = self.thawed.load(Ordering::SeqCst);
        if self.writes == Writes::Stall || (thawed && self.freezer == Freezer::ThawsSilent) {
            std::future::pending::<()>().await;
        }
        if thawed && self.freezer == Freezer::ThawsBroken {
            return Err(std::io::Error::other(EXECUTOR_GONE).into());
        }
        Ok(Listing {
            entries: Vec::new(),
            truncated: false,
        })
    }
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
