//! Checkpoint storage both binaries share.
//!
//! A save reads a workspace as a stream of [`Change`]s: the entries its
//! manifest lists and the chunks whose bytes it uploads. A restore reads each
//! stored object back through [`open`], which checks it against what the daemon
//! registered before one byte of it is used. [`seal`] is the other half, so the
//! key and nonce handling a save and a restore share lives in one place.
//! [`ChunkStore`] is where a save publishes its changes and a restore reads
//! them back, and `test-util` holds one in memory.

mod chain;
mod change;
mod error;
#[cfg(any(test, feature = "test-util"))]
mod memory;
mod sealed;
mod store;

pub use self::chain::{MAX_ENTRIES, Resolved};
pub use self::change::{Change, ChunkKey};
pub use self::error::{Error, Result, unavailable};
#[cfg(any(test, feature = "test-util"))]
pub use self::memory::MemoryStore;
pub use self::sealed::{MAX_OBJECT_BYTES, cipher, open, seal, stored_length};
pub use self::store::{ChunkSource, ChunkStore, Publication, READ_WINDOW, Stored};
