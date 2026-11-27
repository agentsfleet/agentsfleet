//! Checkpoint storage both binaries share.
//!
//! A save reads a workspace as a stream of [`Change`]s: the entries its
//! manifest lists and the chunks whose bytes it uploads. A restore reads each
//! stored object back through [`open`], which checks it against what the daemon
//! registered before one byte of it is used. [`seal`] is the other half, so the
//! key and nonce handling a save and a restore share lives in one place.

mod change;
mod error;
mod sealed;

pub use self::change::{Change, ChunkKey};
pub use self::error::{Error, Result};
pub use self::sealed::{MAX_OBJECT_BYTES, cipher, open, seal, stored_length};
