//! Snapshots of a running sandbox: taken under a caller-minted identifier,
//! restored in place, and forgotten. The seam states the behaviour every
//! engine that offers them holds to; the conformance suite proves it.
//!
//! A snapshot names what it ran on, and restores nowhere else:
//!
//! ```text
//!   SnapshotRef { id, environment, generation, format }
//!   restore ─► environment the sandbox's? ─ no ─► refused naming `environment`,
//!                     │                              before any chunk is read
//!                    yes ─► the saved tree
//! ```

use af_core::id::Uuid7;

use super::Environment;
use super::volume::Quiesced;
use crate::error::{Result, SnapshotRefusal, snapshot_refused};

/// How a snapshot's generation holds the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapshotFormat {
    /// As files: entries and the chunks of each file.
    Files,
}

/// One snapshot a sandbox took: the identifier its caller minted, what the
/// sandbox ran on, and the generation its store committed it as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRef {
    id: Uuid7,
    environment: Environment,
    generation: u64,
    format: SnapshotFormat,
}

impl SnapshotRef {
    /// The snapshot named `id`, taken on `environment` and committed as
    /// `generation` in `format`.
    #[must_use]
    pub const fn new(
        id: Uuid7,
        environment: Environment,
        generation: u64,
        format: SnapshotFormat,
    ) -> Self {
        Self {
            id,
            environment,
            generation,
            format,
        }
    }

    /// The identifier it was taken under.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        &self.id
    }

    /// What the sandbox it was taken on ran on.
    #[must_use]
    pub const fn environment(&self) -> &Environment {
        &self.environment
    }

    /// The generation its store committed it as.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// How it holds the workspace.
    #[must_use]
    pub const fn format(&self) -> SnapshotFormat {
        self.format
    }

    /// Nothing when it was taken on `environment`.
    ///
    /// # Errors
    /// It names another engine, root or guest agent, refused naming
    /// [`SnapshotRefusal::Environment`].
    pub fn restores_on(&self, environment: &Environment) -> Result<()> {
        if self.environment == *environment {
            Ok(())
        } else {
            Err(snapshot_refused(SnapshotRefusal::Environment))
        }
    }
}

/// What a sandbox whose engine offers snapshots does with them.
///
/// ```text
///   quiesce(parent) ─► the fleet runs on ─► publish(quiesced, id)   the store's
///                                       └─► the daemon's publication, then release
///   take(id) = quiesce(the last take) + publish
/// ```
#[async_trait::async_trait]
pub trait Snapshots: Send + Sync {
    /// Holds tenant code still, fixes what changed since `parent`, or
    /// everything when it names none, and returns once tenant code runs on:
    /// what a publication reads, after it.
    ///
    /// # Errors
    /// The hold could not be taken; nothing is held.
    async fn quiesce(&self, parent: Option<&Uuid7>) -> Result<Box<dyn Quiesced + '_>>;

    /// Publishes what `quiesced` fixed under `id` into the store the
    /// sandbox's build was handed, then releases it.
    ///
    /// # Errors
    /// The build was handed no store, or the publication failed; nothing is
    /// kept under `id`.
    async fn publish<'a>(
        &'a self,
        quiesced: Box<dyn Quiesced + 'a>,
        id: Uuid7,
    ) -> Result<SnapshotRef>;

    /// Saves the sandbox's files under `id`, a delta on the last take, and
    /// returns once the save holds.
    ///
    /// # Errors
    /// The save could not be made; nothing is kept under `id`.
    async fn take(&self, id: Uuid7) -> Result<SnapshotRef>;

    /// Replaces the sandbox's files with what `snapshot` saved, and ends every
    /// process that ran before it: nothing of the old environment survives.
    /// It returns once the sandbox's egress plan reads back in force, so no
    /// restore brings back a plan older than the one the run holds.
    ///
    /// # Errors
    /// `snapshot` was taken on another environment, refused before anything
    /// is read; it names none this sandbox's store holds; or the restore
    /// failed.
    async fn restore(&self, snapshot: &SnapshotRef) -> Result<()>;

    /// Drops `snapshot`; a later restore of it is refused.
    ///
    /// # Errors
    /// What it held could not be removed.
    async fn forget(&self, snapshot: &SnapshotRef) -> Result<()>;
}

#[cfg(test)]
mod tests;
