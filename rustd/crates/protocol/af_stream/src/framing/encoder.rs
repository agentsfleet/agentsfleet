//! Reserve frame memory before a service assembles and serializes borrowed output.

use af_wire::protocol::StreamKind;
use serde::Serialize;
use tokio::{sync::OwnedSemaphorePermit, time::Instant};
use tokio_util::sync::CancellationToken;

use super::{Budget, OwnedFrame, writer::Buffer};
use crate::{Result, limits::Length};

/// Room reserved before a payload exists. Nearly every frame fits in it, so
/// it is charged before its bytes are; a larger frame owes the rest, which
/// [`Encoder::finish`] waits for before the frame can be sent.
const RESERVED_BYTES: usize = 2 * 1024 * 1024;

/// A frame being built inside a producer that cannot wait for memory, such
/// as the transaction that chooses an assignment. Dropping it zeroes its
/// bytes and returns its reservation.
pub struct Encoder {
    budget: Budget,
    kind: StreamKind,
    reservation: OwnedSemaphorePermit,
    buffer: Buffer,
    owed: usize,
}

impl std::fmt::Debug for Encoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encoder").finish_non_exhaustive()
    }
}

impl Encoder {
    /// Reserve `RESERVED_BYTES` before invoking a payload producer, so a
    /// pool under pressure delays the producer rather than its frame.
    ///
    /// # Errors
    /// Preserves memory-pressure deadlines and caller cancellation.
    pub async fn reserve(
        budget: &Budget,
        kind: StreamKind,
        stop: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self> {
        let reservation = budget.reserve(kind, RESERVED_BYTES, stop, deadline).await?;
        Ok(Self {
            budget: budget.clone(),
            kind,
            reservation,
            buffer: Buffer::default(),
            owed: 0,
        })
    }

    /// Serialize `value` at its measured length, without waiting. Bytes past
    /// the reserved room are owed and charged by [`Self::finish`].
    ///
    /// # Errors
    /// Refuses a value past its stream's ceiling, or a failed serialization.
    pub fn frame<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let length = Length::measure(value, self.kind)?;
        let mut buffer = Buffer::exact(&length);
        buffer.fill(value)?;
        self.owed = length.bytes.saturating_sub(self.reservation.num_permits());
        self.buffer = buffer;
        Ok(())
    }

    /// Charge whatever [`Self::frame`] owed, then hand the frame and its
    /// charge over together.
    ///
    /// # Errors
    /// Refuses an empty frame; preserves memory-pressure deadlines and
    /// caller cancellation while waiting for an owed charge.
    pub async fn finish(
        mut self,
        stop: &CancellationToken,
        deadline: Instant,
    ) -> Result<OwnedFrame> {
        if self.owed > 0 {
            let owed = self
                .budget
                .reserve(self.kind, self.owed, stop, deadline)
                .await?;
            self.reservation.merge(owed);
        }
        self.buffer.finish(self.reservation, self.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::{Encoder, RESERVED_BYTES};
    use crate::Budget;
    use af_wire::protocol::{MESSAGE_BYTES_MAX, StreamKind};
    use tokio::time::Instant;
    use tokio_util::sync::CancellationToken;

    fn deadline() -> Instant {
        Instant::now() + std::time::Duration::from_secs(5)
    }

    #[tokio::test]
    async fn a_small_frame_keeps_only_its_bytes_of_the_reserved_room() -> crate::Result<()> {
        let budget = Budget::default();
        let available = budget.available(StreamKind::Work);
        let mut encoder = Encoder::reserve(
            &budget,
            StreamKind::Work,
            &CancellationToken::new(),
            deadline(),
        )
        .await?;
        assert_eq!(
            available - budget.available(StreamKind::Work),
            RESERVED_BYTES
        );
        encoder.frame(&"assignment")?;
        let frame = encoder
            .finish(&CancellationToken::new(), deadline())
            .await?;
        assert_eq!(frame.as_bytes(), br#""assignment""#);
        assert_eq!(available - budget.available(StreamKind::Work), 12);
        drop(frame);
        assert_eq!(budget.available(StreamKind::Work), available);
        Ok(())
    }

    #[tokio::test]
    async fn a_frame_past_the_reserved_room_is_charged_its_whole_length() -> crate::Result<()> {
        let budget = Budget::default();
        let available = budget.available(StreamKind::Work);
        let value = "x".repeat(RESERVED_BYTES * 3);
        let mut encoder = Encoder::reserve(
            &budget,
            StreamKind::Work,
            &CancellationToken::new(),
            deadline(),
        )
        .await?;
        encoder.frame(&value)?;
        let frame = encoder
            .finish(&CancellationToken::new(), deadline())
            .await?;
        assert_eq!(frame.as_bytes().len(), RESERVED_BYTES * 3 + 2);
        assert_eq!(
            available - budget.available(StreamKind::Work),
            RESERVED_BYTES * 3 + 2
        );
        drop(frame);
        assert_eq!(budget.available(StreamKind::Work), available);
        Ok(())
    }

    #[tokio::test]
    async fn an_owed_charge_waits_for_room_and_is_cancellable() -> crate::Result<()> {
        let budget = Budget::new(RESERVED_BYTES, RESERVED_BYTES * 2)?;
        let stop = CancellationToken::new();
        let mut encoder = Encoder::reserve(&budget, StreamKind::Work, &stop, deadline()).await?;
        encoder.frame(&"x".repeat(RESERVED_BYTES * 3))?;
        let finishing = encoder.finish(&stop, deadline());
        tokio::pin!(finishing);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                finishing.as_mut().poll(cx).is_pending()
            ))
            .await
        );
        stop.cancel();
        let refused = finishing
            .await
            .err()
            .ok_or(crate::error::ErrorKind::Protocol)?;
        assert_eq!(refused.reason(), crate::Reason::Cancelled);
        assert_eq!(budget.available(StreamKind::Work), RESERVED_BYTES * 2);
        Ok(())
    }

    #[tokio::test]
    async fn a_value_past_the_frame_ceiling_is_refused_and_owes_nothing() -> crate::Result<()> {
        let budget = Budget::default();
        let available = budget.available(StreamKind::Work);
        let mut encoder = Encoder::reserve(
            &budget,
            StreamKind::Work,
            &CancellationToken::new(),
            deadline(),
        )
        .await?;
        let refused = encoder
            .frame(&"x".repeat(MESSAGE_BYTES_MAX))
            .err()
            .ok_or(crate::error::ErrorKind::Protocol)?;
        assert_eq!(refused.reason(), crate::Reason::Protocol);
        drop(encoder);
        assert_eq!(budget.available(StreamKind::Work), available);
        Ok(())
    }

    #[tokio::test]
    async fn finishing_without_a_frame_is_refused() -> crate::Result<()> {
        let budget = Budget::default();
        let available = budget.available(StreamKind::Work);
        let encoder = Encoder::reserve(
            &budget,
            StreamKind::Work,
            &CancellationToken::new(),
            deadline(),
        )
        .await?;
        let refused = encoder
            .finish(&CancellationToken::new(), deadline())
            .await
            .err()
            .ok_or(crate::error::ErrorKind::Protocol)?;
        assert_eq!(refused.reason(), crate::Reason::Protocol);
        assert!(
            std::error::Error::source(&refused)
                .is_some_and(<dyn std::error::Error>::is::<garde::Report>)
        );
        assert_eq!(budget.available(StreamKind::Work), available);
        Ok(())
    }
}
