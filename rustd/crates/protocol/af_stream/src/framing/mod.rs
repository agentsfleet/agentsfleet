//! Length-delimited JSON with memory reserved before decoder allocation.

mod budget;
mod encoder;
mod memory;
mod reader;
mod writer;

use serde::Deserialize;
use tokio::sync::OwnedSemaphorePermit;
use zeroize::Zeroizing;

use crate::Result;

pub use budget::Budget;
pub use encoder::Encoder;
pub use memory::{CONTROL_MEMORY_ENV, DATA_MEMORY_ENV, MemoryLimits, MemorySettings};
pub use reader::Reader;
pub use writer::{encode, write};

/// An owned payload whose memory remains charged until its last use ends.
pub struct OwnedFrame {
    bytes: Zeroizing<Vec<u8>>,
    reservation: OwnedSemaphorePermit,
}

impl OwnedFrame {
    fn allocate(
        length: &garde::Valid<crate::limits::Length>,
        reservation: OwnedSemaphorePermit,
    ) -> Self {
        Self {
            bytes: Zeroizing::new(vec![0; length.bytes]),
            reservation,
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

    /// Move the allocation charge without copying or releasing its zeroing payload owner.
    ///
    /// # Errors
    /// Refuses unavailable destination capacity immediately, keeping the original charge intact.
    pub fn try_rebudget(
        &mut self,
        target: &Budget,
        kind: af_wire::protocol::StreamKind,
    ) -> Result<()> {
        let reservation = target.try_reserve(kind, self.bytes.capacity())?;
        self.reservation = reservation;
        Ok(())
    }
}

impl std::fmt::Debug for OwnedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedFrame")
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_wire::protocol::StreamKind;
    use tokio::time::Instant;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn transferring_a_frame_preserves_its_allocation_and_releases_both_charges() -> Result<()>
    {
        let original = Budget::default();
        let available = original.available(StreamKind::Work);
        let mut frame = encode(
            &"retained payload",
            &original,
            StreamKind::Work,
            &CancellationToken::new(),
            Instant::now() + std::time::Duration::from_secs(1),
        )
        .await?;
        let pointer = frame.as_bytes().as_ptr();
        let charged = available - original.available(StreamKind::Work);
        let target = Budget::new(0, charged)?;
        frame.try_rebudget(&target, StreamKind::Work)?;
        assert_eq!(frame.as_bytes().as_ptr(), pointer);
        assert_eq!(frame.decode::<&str>()?, "retained payload");
        assert_eq!(original.available(StreamKind::Work), available);
        assert_eq!(target.available(StreamKind::Reports), 0);
        drop(frame);
        assert_eq!(target.available(StreamKind::Reports), charged);
        Ok(())
    }

    #[tokio::test]
    async fn an_insufficient_destination_keeps_the_original_capacity_charged() -> Result<()> {
        let original = Budget::default();
        let available = original.available(StreamKind::Work);
        let mut frame = encode(
            &"retained payload",
            &original,
            StreamKind::Work,
            &CancellationToken::new(),
            Instant::now() + std::time::Duration::from_secs(1),
        )
        .await?;
        let pointer = frame.as_bytes().as_ptr();
        let charged = available - original.available(StreamKind::Work);
        let target = Budget::new(0, charged - 1)?;
        let failure = frame
            .try_rebudget(&target, StreamKind::Work)
            .err()
            .ok_or(crate::error::ErrorKind::Protocol)?;
        assert_eq!(failure.reason(), crate::Reason::Protocol);
        assert_eq!(
            failure.code(),
            af_core::error_code::INTERNAL_OPERATION_FAILED
        );
        assert!(std::error::Error::source(&failure).is_some_and(|source| {
            source.downcast_ref::<tokio::sync::TryAcquireError>()
                == Some(&tokio::sync::TryAcquireError::NoPermits)
        }));
        assert_eq!(frame.as_bytes().as_ptr(), pointer);
        assert_eq!(original.available(StreamKind::Work), available - charged);
        assert_eq!(target.available(StreamKind::Work), charged - 1);
        drop(frame);
        assert_eq!(original.available(StreamKind::Work), available);
        Ok(())
    }
}
