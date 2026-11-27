//! Where a save's changes are published, and where a restore reads them back.
//!
//! ```text
//!   publish(id, parent) ─► add(change) … ─► commit ─► generation
//!   tree(id) ─► Stored { generation, tree }     read(object) ─► plaintext
//! ```
//!
//! The supervisor's store publishes over signed links and reads through
//! [`crate::open`]'s check; the in-memory one behind `test-util` holds
//! everything in the process. Neither hands a key or a link to anything it
//! reads for or writes from: it lives on the host, never in a guest.

use std::fmt;

use af_core::id::Uuid7;
use zeroize::Zeroizing;

use crate::{Change, Resolved, Result};

/// One saved tree, as the store that holds it names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// The generation its publication committed as.
    pub generation: u64,
    /// The tree it resolves to, its chain applied.
    pub tree: Resolved,
}

/// What a restore reads a saved tree back from.
#[async_trait::async_trait]
pub trait ChunkSource: Send + Sync + fmt::Debug {
    /// The tree saved under `id`.
    ///
    /// # Errors
    /// The store names nothing under `id`, or could not be read.
    async fn tree(&self, id: &Uuid7) -> Result<Stored>;

    /// The plaintext of `object`, once it passed its check.
    ///
    /// # Errors
    /// The object is not stored, or failed its check.
    async fn read(&self, object: &str) -> Result<Zeroizing<Vec<u8>>>;
}

/// A [`ChunkSource`] a save publishes into.
#[async_trait::async_trait]
pub trait ChunkStore: ChunkSource {
    /// Opens the publication of a tree to be named `id`: the changes since
    /// `parent`, or the whole tree when it names none. Nothing is named `id`
    /// until [`Publication::commit`] answers.
    ///
    /// # Errors
    /// `parent` names nothing this store holds, or `id` is taken.
    async fn publish(
        &self,
        id: &Uuid7,
        parent: Option<&Uuid7>,
    ) -> Result<Box<dyn Publication + '_>>;

    /// Stops naming `id`, so what nothing else names may be collected.
    ///
    /// # Errors
    /// The store names nothing under `id`.
    async fn forget(&self, id: &Uuid7) -> Result<()>;
}

/// One publication under way; dropped uncommitted, it names nothing.
#[async_trait::async_trait]
pub trait Publication: Send {
    /// Takes the next change: an entry, or a chunk of the file entry before it.
    ///
    /// # Errors
    /// A chunk with no file entry before it, out of order, or not stored.
    async fn add(&mut self, change: Change) -> Result<()>;

    /// Names the tree, and answers the generation it committed as.
    ///
    /// # Errors
    /// The commit was refused; nothing is named.
    async fn commit(self: Box<Self>) -> Result<u64>;
}
