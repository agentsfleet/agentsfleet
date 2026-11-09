//! Length-delimited JSON with memory reserved before decoder allocation.

mod budget;
mod memory;
mod encoder;
mod reader;
mod writer;

use serde::Deserialize;
use tokio::sync::OwnedSemaphorePermit;
use zeroize::Zeroizing;

use crate::Result;

pub use budget::Budget;
pub use memory::{CONTROL_MEMORY_ENV, DATA_MEMORY_ENV, MemoryLimits, MemorySettings};
pub use encoder::Encoder;
pub use reader::Reader;
pub use writer::{encode, write};

/// An owned payload whose memory remains charged until its last use ends.
pub struct OwnedFrame {
    bytes: Zeroizing<Vec<u8>>,
    _reservation: OwnedSemaphorePermit,
}

impl OwnedFrame {
    fn allocate(
        length: &garde::Valid<crate::limits::Length>,
        reservation: OwnedSemaphorePermit,
    ) -> Self {
        Self {
            bytes: Zeroizing::new(vec![0; length.bytes]),
            _reservation: reservation,
        }
    }

    /// Decode while retaining the backing bytes for borrowed wire fields.
    ///
    /// # Errors
    /// Returns a JSON failure; callers still validate the decoded message.
    pub fn decode<'a, T: Deserialize<'a>>(&'a self) -> Result<T> {
        Ok(serde_json::from_slice(&self.bytes)?)
    }

    /// Payload bytes, excluding the four-byte prefix.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl std::fmt::Debug for OwnedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedFrame")
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
