//! Snapshots of a running sandbox: taken under a caller-minted identifier,
//! restored in place, and forgotten. The seam states the behaviour every
//! engine that offers them holds to; the conformance suite proves it, and no
//! mechanism behind it is designed here.

use af_core::id::Uuid7;

use crate::error::Result;

/// One snapshot a sandbox took, named by the identifier its caller minted.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotRef {
    id: Uuid7,
}

impl SnapshotRef {
    /// The snapshot named `id`; a sandbox that never took it refuses it.
    #[must_use]
    pub const fn new(id: Uuid7) -> Self {
        Self { id }
    }

    /// The identifier it was taken under.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        &self.id
    }
}

/// What a sandbox whose engine offers snapshots does with them.
#[async_trait::async_trait]
pub trait Snapshots: Send + Sync {
    /// Saves the sandbox's files under `id`, and returns once the save holds.
    ///
    /// # Errors
    /// The save could not be made; nothing is kept under `id`.
    async fn take(&self, id: Uuid7) -> Result<SnapshotRef>;

    /// Replaces the sandbox's files with what `snapshot` saved, and ends every
    /// process that ran before it: nothing of the old environment survives.
    ///
    /// # Errors
    /// `snapshot` names none this sandbox holds, or the restore failed.
    async fn restore(&self, snapshot: &SnapshotRef) -> Result<()>;

    /// Drops `snapshot`; a later restore of it is refused.
    ///
    /// # Errors
    /// What it held could not be removed.
    async fn forget(&self, snapshot: &SnapshotRef) -> Result<()>;
}
