//! Reserve frame memory before a service assembles and serializes borrowed output.

use af_wire::protocol::StreamKind;
use tokio::{sync::OwnedSemaphorePermit, time::Instant};
use tokio_util::sync::CancellationToken;

use super::{
    Budget, OwnedFrame,
    writer::{Buffer, ENCODING_BYTES},
};
use crate::Result;

/// A bounded writer whose allocation stays charged through payload assembly.
/// Dropping an unfinished encoder zeroes its bytes and returns its reservation.
pub struct Encoder {
    buffer: Buffer,
    reservation: OwnedSemaphorePermit,
}

impl std::fmt::Debug for Encoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encoder").finish_non_exhaustive()
    }
}

impl Encoder {
    /// Reserve serialization's peak allocation before invoking a payload producer.
    ///
    /// # Errors
    /// Preserves memory-pressure deadlines and caller cancellation.
    pub async fn reserve(
        budget: &Budget,
        kind: StreamKind,
        stop: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self> {
        let reservation = budget.reserve(kind, ENCODING_BYTES, stop, deadline).await?;
        Ok(Self {
            buffer: Buffer::default(),
            reservation,
        })
    }

    /// Transfer the serialized payload and its retained allocation charge together.
    ///
    /// # Errors
    /// Refuses an empty or oversized payload.
    pub fn finish(self) -> Result<OwnedFrame> {
        self.buffer.finish(self.reservation)
    }
}

impl std::io::Write for Encoder {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        std::io::Write::write(&mut self.buffer, bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.buffer)
    }
}
