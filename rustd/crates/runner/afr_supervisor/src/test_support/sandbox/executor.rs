//! A fake executor has explicit recovery proof and no hidden host work.

use super::{EXECUTOR_GONE, Parking};
use afr_executor::{Executor, FileContent, Imported, Listing, Process, ProcessId, Spawn};
use bytes::Bytes;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use std::io::{Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _};
use tokio::sync::mpsc;

/// An executor that refuses to spawn, reports the files written into it,
/// imports and exports over its workspace directory with the executor's own
/// tree functions, and answers everything else emptily, until its sandbox's
/// parking says not.
#[derive(Debug, Default)]
pub(super) struct FakeExecutor {
    pub(super) written: Option<mpsc::UnboundedSender<(String, Bytes)>>,
    /// The directory it serves `import` and `export` over.
    pub(super) workspace: Option<PathBuf>,
    /// What its sandbox does when resumed.
    pub(super) parking: Parking,
    /// Whether its sandbox was resumed.
    pub(super) resumed: AtomicBool,
}

#[async_trait::async_trait]
impl Executor for FakeExecutor {
    async fn recovery_idle(&self) -> afr_executor::Result<bool> {
        if self.parking == Parking::ExecutorUnproved {
            return Err(std::io::Error::other(EXECUTOR_GONE).into());
        }
        Ok(self.parking != Parking::BusyExecutor)
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
        let resumed = self.resumed.load(Ordering::SeqCst);
        if resumed && self.parking == Parking::ResumesSilent {
            std::future::pending::<()>().await;
        }
        if resumed && self.parking == Parking::ResumesBroken {
            return Err(std::io::Error::other(EXECUTOR_GONE).into());
        }
        Ok(Listing {
            entries: Vec::new(),
            truncated: false,
        })
    }

    async fn import(
        &self,
        at: &str,
        tree: &mut (dyn AsyncRead + Send + Unpin),
    ) -> afr_executor::Result<Imported> {
        let root = self.served()?;
        let mut spool = tokio::fs::File::from_std(tempfile::tempfile()?);
        tokio::io::copy(tree, &mut spool).await?;
        let spool = spool.into_std().await;
        let dir = Dir::open_ambient_dir(root, ambient_authority())?;
        afr_executor::tree::unpack(&dir, Path::new(at), &spool)
    }

    async fn export(
        &self,
        paths: &[&str],
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> afr_executor::Result<()> {
        let dir = Dir::open_ambient_dir(self.served()?, ambient_authority())?;
        let spool = tempfile::tempfile()?;
        let inside: Vec<_> = paths.iter().map(Path::new).collect();
        afr_executor::tree::pack(&dir, &inside, &spool)?;
        let mut rewind = &spool;
        rewind.seek(SeekFrom::Start(0))?;
        tokio::io::copy(&mut tokio::fs::File::from_std(spool), into).await?;
        Ok(into.flush().await?)
    }
}

impl FakeExecutor {
    /// The directory it serves, or the refusal an executor with no file
    /// calls answers.
    fn served(&self) -> afr_executor::Result<&Path> {
        self.workspace
            .as_deref()
            .ok_or_else(|| std::io::Error::other("this fake serves no workspace").into())
    }
}
