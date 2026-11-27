//! A daemon checkpoint read as a chunk source, so a run's restore lands
//! through the engine as any snapshot does, behind the same environment and
//! plan checks.
//!
//! ```text
//!   restore_workspace(sandbox, spools, ref, source)   environment? first
//!     tree(id)      the custody's manifests, each checked against its root
//!     read_window(objects)  renew ─► authorize once ─► GET each ─► open ─► plaintext
//! ```
//!
//! The engine names at most [`af_chunk_store::READ_WINDOW`] objects to a
//! window, so that many GETs are in flight under one authorization.
//!
//! A durable task's custody and a turn's read the same way under their own
//! commands. The engine sees only that the source could not answer; the
//! custody's own failure stays here, and is what the caller is answered with.

use std::fmt;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use af_chunk_store::{ChunkSource, Resolved, Stored};
use af_core::id::Uuid7;
use af_wire::durable_task::CheckpointEntry;
use afr_engine::{Environment, Sandbox, SnapshotFormat, SnapshotRef};
use zeroize::Zeroizing;

use crate::{Result, error};

mod task;

pub(super) use self::task::TaskReading;

/// Why a restore failed after the engine took it, with no custody failure.
const DETAIL_LANDED: &str = "the checkpoint would not restore into the sandbox";
/// Why a tree under another identifier than the checkpoint's was refused.
const DETAIL_OTHER: &str = "checkpoint restore asked for another checkpoint";
/// Why a read the custody answered with no plaintext was refused.
const DETAIL_ABSENT: &str = "checkpoint restore object is absent";

/// Where a restore lands: a sandbox, and where its spools go on the host.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Landing<'a> {
    /// The fresh sandbox the restored tree lands in.
    pub(crate) sandbox: &'a dyn Sandbox,
    /// Where the packed tree waits on its way in: only the runner reaches it.
    pub(crate) spools: &'a Path,
}

/// What a checkpoint's restore custody does for the source that reads it.
pub(super) trait Custody: Send {
    /// The tree the checkpoint resolves to, each manifest checked against its
    /// root and every link inside the workspace.
    fn tree(&mut self) -> impl Future<Output = Result<Resolved>> + Send;

    /// Each of `objects`' plaintext in their order, a repeated object
    /// included: authorized once under the restore pin, fetched together and
    /// each opened against its registration.
    fn read(
        &mut self,
        objects: &[String],
    ) -> impl Future<Output = Result<Vec<Zeroizing<Vec<u8>>>>> + Send;
}

/// A checkpoint's custody as the engine reads it, and the first failure the
/// custody answered.
struct DaemonSource<C> {
    checkpoint: Uuid7,
    generation: u64,
    custody: tokio::sync::Mutex<C>,
    failure: Mutex<Option<crate::Error>>,
}

impl<C> fmt::Debug for DaemonSource<C> {
    /// Only which checkpoint it reads: its custody holds keys.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DaemonSource")
            .field("checkpoint", &self.checkpoint)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl<C: Custody> ChunkSource for DaemonSource<C> {
    async fn tree(&self, id: &Uuid7) -> af_chunk_store::Result<Stored> {
        if *id != self.checkpoint {
            return Err(self.failed(error::config(DETAIL_OTHER)));
        }
        let tree = self.custody.lock().await.tree().await;
        Ok(Stored {
            generation: self.generation,
            tree: tree.map_err(|failure| self.failed(failure))?,
        })
    }

    async fn read(&self, object: &str) -> af_chunk_store::Result<Zeroizing<Vec<u8>>> {
        let mut read = self.read_window(&[object.to_owned()]).await?;
        let absent = || self.failed(error::config(DETAIL_ABSENT));
        read.pop().ok_or_else(absent)
    }

    async fn read_window(
        &self,
        objects: &[String],
    ) -> af_chunk_store::Result<Vec<Zeroizing<Vec<u8>>>> {
        let read = self.custody.lock().await.read(objects).await;
        read.map_err(|failure| self.failed(failure))
    }
}

impl<C> DaemonSource<C> {
    /// Keeps `failure`, unless an earlier one is kept, and tells the engine
    /// only that the source could not answer.
    fn failed(&self, failure: crate::Error) -> af_chunk_store::Error {
        let code = failure.code();
        self.failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(failure);
        af_chunk_store::unavailable(code)
    }
}

/// Lands the checkpoint `custody` reads, committed as `generation` under
/// `checkpoint` in `taken`, in `landing`'s sandbox through its engine, and
/// hands the custody back. The engine refuses a sandbox on any environment
/// but `taken`, naming `environment`, before any chunk is read.
///
/// # Errors
/// The custody's own first failure, when it answered one; otherwise the
/// engine's refusal or failure.
pub(super) async fn land<C: Custody>(
    custody: C,
    (checkpoint, generation): (Uuid7, u64),
    taken: Environment,
    landing: Landing<'_>,
) -> Result<C> {
    let snapshot = SnapshotRef::new(checkpoint.clone(), taken, generation, SnapshotFormat::Files);
    let source = DaemonSource {
        checkpoint,
        generation,
        custody: tokio::sync::Mutex::new(custody),
        failure: Mutex::new(None),
    };
    let landed =
        afr_engine::restore_workspace(landing.sandbox, landing.spools, &snapshot, &source).await;
    let DaemonSource {
        custody, failure, ..
    } = source;
    let failure = failure.into_inner().unwrap_or_else(PoisonError::into_inner);
    failure.map_or_else(
        || {
            landed
                .map(|()| custody.into_inner())
                .map_err(|refused| error::boundary(DETAIL_LANDED, refused))
        },
        Err,
    )
}

/// Holds each link of `tree` inside the workspace, alone and together.
///
/// # Errors
/// A target leaves the workspace, or the links form a cycle or escape.
pub(super) fn links_inside(tree: &Resolved) -> Result<()> {
    super::links::validate(tree.entries().values().filter_map(|entry| match entry {
        CheckpointEntry::Symlink { path, target } => Some((path.as_str(), target.as_str())),
        CheckpointEntry::File(_)
        | CheckpointEntry::Directory { .. }
        | CheckpointEntry::Remove { .. } => None,
    }))
}

#[cfg(test)]
mod tests;
