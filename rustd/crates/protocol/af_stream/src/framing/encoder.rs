//! Serialize borrowed output inside a producer that cannot wait, then charge
//! the frame its exact length once.

use af_wire::protocol::StreamKind;
use serde::Serialize;
use tokio::{sync::OwnedSemaphorePermit, time::Instant};
use tokio_util::sync::CancellationToken;

use super::{Budget, OwnedFrame, writer::Buffer};
use crate::{Result, limits::Length};

/// A frame being built inside a producer that cannot wait for memory, such
/// as the transaction that chooses an assignment. Dropping it zeroes its
/// bytes and returns its charge.
///
/// It never holds part of a charge while it waits for the rest: a frame is
/// charged its whole length in one reservation, so encoders waiting on a
/// busy pool cannot each hold a slice of it and starve one another, or a
/// small frame queued behind them.
pub struct Encoder {
    budget: Budget,
    kind: StreamKind,
    framed: Option<Framed>,
}

/// A serialized frame and, when the pool had room at once, its charge.
struct Framed {
    buffer: Buffer,
    length: usize,
    charge: Option<OwnedSemaphorePermit>,
}

impl std::fmt::Debug for Encoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encoder").finish_non_exhaustive()
    }
}

impl Encoder {
    /// An encoder that has charged nothing yet.
    #[must_use]
    pub fn new(budget: &Budget, kind: StreamKind) -> Self {
        Self {
            budget: budget.clone(),
            kind,
            framed: None,
        }
    }

    /// Serialize `value` at its measured length, without waiting. The frame
    /// is charged its exact length now when the pool has room; otherwise
    /// [`Self::finish`] waits for that charge.
    ///
    /// # Errors
    /// Refuses a value past its stream's ceiling, or a failed serialization.
    pub fn frame<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let length = Length::measure(value, self.kind)?;
        // Dropped before a replacement is allocated, so its charge returns first.
        self.framed = None;
        let charge = self.budget.try_reserve(self.kind, length.bytes).ok();
        let mut buffer = Buffer::exact(&length);
        buffer.fill(value)?;
        self.framed = Some(Framed {
            buffer,
            length: length.bytes,
            charge,
        });
        Ok(())
    }

    /// Charge the frame its exact length, when [`Self::frame`] could not,
    /// then hand the frame and its charge over together.
    ///
    /// # Errors
    /// Refuses an empty frame; preserves memory-pressure deadlines and
    /// caller cancellation while waiting for the charge.
    pub async fn finish(self, stop: &CancellationToken, deadline: Instant) -> Result<OwnedFrame> {
        let Some(framed) = self.framed else {
            Length::prove(0, self.kind)?;
            return Err(crate::error::ErrorKind::Protocol.into());
        };
        let charge = match framed.charge {
            Some(charge) => charge,
            None => {
                self.budget
                    .reserve(self.kind, framed.length, stop, deadline)
                    .await?
            }
        };
        framed.buffer.finish(charge, self.kind)
    }
}

#[cfg(test)]
#[path = "encoder_tests.rs"]
mod tests;
