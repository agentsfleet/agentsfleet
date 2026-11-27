//! A fake guest agent has explicit recovery proof and no hidden host work.

use super::{AGENT_GONE, FREEZE, Parking, THAW};
use af_guest_wire::protocol::PinWalked;
use af_guest_wire::tree::Pins;
use af_guest_wire::{FileContent, Imported, Listing, Sizes};
use afr_guest_client::{GuestAgent, Process, ProcessId, Spawn};
use bytes::Bytes;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use std::io::{Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _};
use tokio::sync::mpsc;

/// A guest agent that refuses to spawn, reports the files written into it,
/// imports and exports over its workspace directory with the guest agent's own
/// tree functions, and answers everything else emptily, until its sandbox's
/// parking says not.
#[derive(Debug, Default)]
pub(super) struct FakeGuest {
    pub(super) written: Option<mpsc::UnboundedSender<(String, Bytes)>>,
    /// The directory it serves `import` and `export` over.
    pub(super) workspace: Option<PathBuf>,
    /// What its sandbox does when resumed.
    pub(super) parking: Parking,
    /// Whether its sandbox was resumed.
    pub(super) resumed: AtomicBool,
    /// Whether a turn listed a directory through it: the last of the calls
    /// a fake turn makes.
    pub(super) served: AtomicBool,
    /// Whether it has a tenant leaf to freeze.
    pub(super) freezes: bool,
    /// Where it reports each freeze and thaw.
    pub(super) steps: Option<mpsc::UnboundedSender<&'static str>>,
    /// Each save pin's identities, kept as the guest agent keeps them: a
    /// test seeds them by walking and releasing a pin through it.
    pub(super) pins: std::sync::Arc<Mutex<Pins>>,
}

#[async_trait::async_trait]
impl GuestAgent for FakeGuest {
    async fn recovery_idle(&self) -> af_guest_wire::Result<bool> {
        if self.parking == Parking::AgentUnproved {
            return Err(std::io::Error::other(AGENT_GONE).into());
        }
        Ok(self.parking != Parking::BusyAgent)
    }

    /// Its calls are what the busy-agent parking holds; a tenant process
    /// is its sandbox's to report.
    async fn calls_idle(&self) -> af_guest_wire::Result<bool> {
        self.recovery_idle().await
    }

    async fn freeze_tenant(&self) -> af_guest_wire::Result<()> {
        self.leaf(FREEZE)
    }

    async fn thaw_tenant(&self) -> af_guest_wire::Result<()> {
        self.leaf(THAW)
    }

    async fn spawn(&self, _spawn: &Spawn) -> af_guest_wire::Result<Process> {
        Err(std::io::Error::other("no processes here").into())
    }

    async fn write(&self, _process: ProcessId, _data: Bytes) -> af_guest_wire::Result<()> {
        Ok(())
    }

    async fn kill(&self, _process: ProcessId) -> af_guest_wire::Result<()> {
        Ok(())
    }

    async fn read_file(&self, _path: &str, _max_bytes: u64) -> af_guest_wire::Result<FileContent> {
        Ok(FileContent {
            data: Bytes::new(),
            truncated: false,
        })
    }

    async fn write_file(&self, path: &str, data: Bytes) -> af_guest_wire::Result<()> {
        if let Some(written) = &self.written {
            let _reader_gone = written.send((path.to_owned(), data));
        }
        Ok(())
    }

    async fn list_dir(&self, _path: &str) -> af_guest_wire::Result<Listing> {
        self.served.store(true, Ordering::SeqCst);
        let resumed = self.resumed.load(Ordering::SeqCst);
        if resumed && self.parking == Parking::ResumesSilent {
            std::future::pending::<()>().await;
        }
        if resumed && self.parking == Parking::ResumesBroken {
            return Err(std::io::Error::other(AGENT_GONE).into());
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
    ) -> af_guest_wire::Result<Imported> {
        let root = self.served()?;
        let mut spool = tokio::fs::File::from_std(tempfile::tempfile()?);
        tokio::io::copy(tree, &mut spool).await?;
        let spool = spool.into_std().await;
        let dir = Dir::open_ambient_dir(root, ambient_authority())?;
        af_guest_wire::tree::unpack(&dir, Path::new(at), &spool)
    }

    async fn export(
        &self,
        paths: &[&str],
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> af_guest_wire::Result<()> {
        let dir = Dir::open_ambient_dir(self.served()?, ambient_authority())?;
        let spool = tempfile::tempfile()?;
        let inside: Vec<_> = paths.iter().map(Path::new).collect();
        af_guest_wire::tree::pack(&dir, &inside, &spool)?;
        let mut rewind = &spool;
        rewind.seek(SeekFrom::Start(0))?;
        tokio::io::copy(&mut tokio::fs::File::from_std(spool), into).await?;
        Ok(into.flush().await?)
    }

    /// Walks its workspace with the guest agent's own pin record and packs
    /// what changed into `into`.
    async fn pin_walk(
        &self,
        parent: Option<&str>,
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> af_guest_wire::Result<PinWalked> {
        let dir = Dir::open_ambient_dir(self.served()?, ambient_authority())?;
        let walked = self
            .pins
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .walk(&dir, parent, Sizes::DEFAULT.max_frame_bytes)?;
        let spool = tempfile::tempfile()?;
        let roots: Vec<_> = walked.roots.iter().map(PathBuf::as_path).collect();
        af_guest_wire::tree::pack(&dir, &roots, &spool)?;
        let mut rewind = &spool;
        rewind.seek(SeekFrom::Start(0))?;
        tokio::io::copy(&mut tokio::fs::File::from_std(spool), into).await?;
        into.flush().await?;
        Ok(PinWalked {
            pin: walked.pin,
            transfer_id: walked.pin,
            delta: walked.delta,
            removed: walked.removed,
            entries: walked.entries,
            changed_bytes: walked.changed_bytes,
        })
    }

    async fn pin_release(&self, pin: u64, generation: Option<&str>) -> af_guest_wire::Result<()> {
        self.pins
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .release(pin, generation)
    }
}

impl FakeGuest {
    /// Reports `step` on its tenant leaf, or refuses it as a guest agent with
    /// none does.
    fn leaf(&self, step: &'static str) -> af_guest_wire::Result<()> {
        if !self.freezes {
            return Err(af_guest_wire::error::no_tenant_leaf_refused());
        }
        if let Some(steps) = &self.steps {
            let _reader_gone = steps.send(step);
        }
        Ok(())
    }

    /// The directory it serves, or the refusal a guest agent with no file
    /// calls answers.
    fn served(&self) -> af_guest_wire::Result<&Path> {
        self.workspace
            .as_deref()
            .ok_or_else(|| std::io::Error::other("this fake serves no workspace").into())
    }
}
