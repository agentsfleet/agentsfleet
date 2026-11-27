//! What a save reads out of a pinned workspace.

use af_wire::durable_task::CheckpointEntry;
use zeroize::Zeroizing;

/// One thing a pin saw change: an entry its manifest lists, or the bytes of
/// one chunk a file entry holds.
///
/// A file's chunks follow its entry, in index order, so a reader that keeps
/// only the entry it is on places every chunk.
pub enum Change {
    /// A directory, a link, a removal, or a file whose `objects` the save
    /// fills in once its chunks are registered.
    Entry(CheckpointEntry),
    /// The plaintext of one chunk, wiped when dropped.
    Chunk {
        /// Where the bytes sit.
        key: ChunkKey,
        /// The chunk's plaintext.
        bytes: Zeroizing<Vec<u8>>,
    },
}

impl std::fmt::Debug for Change {
    /// A chunk's bytes are tenant content, so only where it sits is told.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entry(entry) => f.debug_tuple("Entry").field(entry).finish(),
            Self::Chunk { key, bytes } => f
                .debug_struct("Chunk")
                .field("key", key)
                .field("size", &bytes.len())
                .finish(),
        }
    }
}

/// Where one chunk's bytes sit in the workspace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChunkKey {
    /// The `index`th chunk of the file at `path`.
    File {
        /// The file's path, relative to the workspace.
        path: String,
        /// Which chunk, from 0.
        index: u64,
    },
}
