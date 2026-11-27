//! A saved tree read out of a chunk source and landed in a sandbox's
//! workspace.
//!
//! ```text
//!   environment? ─► tree ─► generation? ─► pack ─► renew ─► import    snapshots
//!   environment? ─► tree ─► generation? ─► empty? ─► pack ─► import   none
//! ```
//!
//! Both refuse a ref from another environment before the source is read, and
//! read every chunk before the workspace changes: a restore that cannot read
//! its tree leaves the workspace as it was. A sandbox whose engine offers
//! snapshots renews its inner environment through them and reads its egress
//! plan back; one with none has a guest agent that cannot renew, so it lands
//! only in a workspace that holds nothing yet.

use std::fs::File;
use std::io;
use std::path::Path;

use af_chunk_store::{ChunkSource, Resolved};
use afr_guest_client::GuestAgent;

use super::{WHOLE, pack};
use crate::engine::{Sandbox, SnapshotRef};
use crate::error::{Result, SnapshotRefusal, snapshot_refused};

/// Why a restore into a sandbox with no snapshots was refused.
const NOT_EMPTY: &str = "a restore lands only in an empty workspace";

/// The tree `snapshot` saved into `source`, once it names `sandbox`'s
/// environment and the generation `source` holds under its identifier.
///
/// # Errors
/// Another environment, refused before `source` is read; another
/// generation; or `source` names nothing under its identifier.
async fn saved(
    sandbox: &dyn Sandbox,
    snapshot: &SnapshotRef,
    source: &dyn ChunkSource,
) -> Result<Resolved> {
    snapshot.restores_on(sandbox.environment())?;
    let stored = source.tree(snapshot.id()).await?;
    if stored.generation != snapshot.generation() {
        return Err(snapshot_refused(SnapshotRefusal::Generation));
    }
    Ok(stored.tree)
}

/// What `snapshot` saved into `source`, read whole and packed under
/// `spools` for `sandbox`'s import.
///
/// # Errors
/// As [`saved`], or a chunk would not read.
pub(super) async fn packed(
    sandbox: &dyn Sandbox,
    spools: &Path,
    snapshot: &SnapshotRef,
    source: &dyn ChunkSource,
) -> Result<File> {
    let tree = saved(sandbox, snapshot, source).await?;
    pack::pack(&tree, source, spools).await
}

/// Imports `packed` over the whole workspace `agent` serves.
///
/// # Errors
/// The guest agent refused the stream.
pub(super) async fn import(agent: &dyn GuestAgent, packed: File) -> Result<()> {
    agent
        .import(WHOLE, &mut tokio::fs::File::from_std(packed))
        .await?;
    Ok(())
}

/// Replaces `sandbox`'s workspace with what `snapshot` saved into `source`.
///
/// A sandbox whose engine offers snapshots restores through them, which
/// renew it and read its egress plan back. One with none takes the tree only
/// into a workspace that holds nothing yet, each chunk spooled under
/// `spools`.
///
/// # Errors
/// As [`crate::Snapshots::restore_from`]; with no snapshots, the workspace
/// already holds an entry, refused before any chunk is read.
pub async fn restore_workspace(
    sandbox: &dyn Sandbox,
    spools: &Path,
    snapshot: &SnapshotRef,
    source: &dyn ChunkSource,
) -> Result<()> {
    if let Some(snapshots) = sandbox.snapshots() {
        return snapshots.restore_from(snapshot, source).await;
    }
    let tree = saved(sandbox, snapshot, source).await?;
    let agent = sandbox.agent();
    let listed = agent.list_dir(WHOLE).await?;
    if !listed.entries.is_empty() || listed.truncated {
        return Err(io::Error::new(io::ErrorKind::DirectoryNotEmpty, NOT_EMPTY).into());
    }
    import(agent, pack::pack(&tree, source, spools).await?).await
}
