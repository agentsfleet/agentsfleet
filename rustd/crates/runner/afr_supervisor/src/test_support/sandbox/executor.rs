//! A fake executor has explicit recovery proof and no hidden host work.

use super::{EXECUTOR_GONE, Freezer};
use afr_executor::{Executor, FileContent, Listing, Process, ProcessId, Spawn};
use bytes::Bytes;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

/// An executor that refuses to spawn, reports the files written into it, and
/// answers everything else emptily, until its sandbox's freezer says not.
#[derive(Debug, Default)]
pub(super) struct FakeExecutor {
    pub(super) written: Option<mpsc::UnboundedSender<(String, Bytes)>>,
    /// What its sandbox does when thawed.
    pub(super) freezer: Freezer,
    /// Whether its sandbox was thawed.
    pub(super) thawed: AtomicBool,
}

#[async_trait::async_trait]
impl Executor for FakeExecutor {
    async fn recovery_idle(&self) -> afr_executor::Result<bool> {
        Ok(self.freezer != Freezer::BusyExecutor)
    }

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
        if let Some(written) = &self.written {
            let _reader_gone = written.send((path.to_owned(), data));
        }
        Ok(())
    }

    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Listing> {
        let thawed = self.thawed.load(Ordering::SeqCst);
        if thawed && self.freezer == Freezer::ThawsSilent {
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
