//! Bound serialization and every in-progress frame write.

use af_wire::protocol::StreamKind;
use garde::Valid;
use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::{Budget, OwnedFrame};
use crate::{Result, Stage, error, limits::Length};

/// A frame's bytes, allocated once at their measured length and never grown,
/// so the allocation is exactly what its reservation charges.
#[derive(Default)]
pub(super) struct Buffer(Zeroizing<Vec<u8>>);

impl Buffer {
    /// Room for exactly `length` bytes: `Vec::with_capacity` gives a byte
    /// vector exactly the capacity it asks for.
    pub(super) fn exact(length: &Valid<Length>) -> Self {
        Self(Zeroizing::new(Vec::with_capacity(length.bytes)))
    }

    /// Serialize `value` into the room measured for it.
    pub(super) fn fill<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        serde_json::to_writer(&mut *self, value)?;
        Ok(())
    }

    /// The frame, charged its allocation: a reservation larger than the
    /// allocation returns the difference to its pool.
    pub(super) fn finish(
        self,
        mut reservation: OwnedSemaphorePermit,
        kind: StreamKind,
    ) -> Result<OwnedFrame> {
        let allocation = Length::prove(self.0.capacity(), kind)?;
        let _unused = reservation.split(reservation.num_permits().saturating_sub(allocation.bytes));
        Ok(OwnedFrame {
            bytes: self.0,
            reservation,
        })
    }
}

impl std::io::Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Growing past the measured room would leave bytes no reservation charges.
        if buf.len() > self.0.capacity() - self.0.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame exceeds its measured length",
            ));
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Measure `value`, reserve exactly those bytes, then serialize into them.
///
/// A value past its stream's ceiling is refused before anything is reserved,
/// and the frame never holds a second, growing copy of itself.
///
/// # Errors
/// Refuses unavailable memory, oversized values and failed serialization.
pub async fn encode<T: Serialize + Sync + ?Sized>(
    value: &T,
    budget: &Budget,
    kind: StreamKind,
    stop: &CancellationToken,
    deadline: Instant,
) -> Result<OwnedFrame> {
    let length = Length::measure(value, kind)?;
    let reservation = budget.reserve(kind, length.bytes, stop, deadline).await?;
    let mut buffer = Buffer::exact(&length);
    buffer.fill(value)?;
    buffer.finish(reservation, kind)
}

/// Write a prefix and at most 16 KiB per data operation under one deadline.
///
/// # Errors
/// Returns the original I/O failure, cancellation or the absolute timeout.
pub async fn write<W: AsyncWrite + Unpin>(
    output: &mut W,
    frame: &OwnedFrame,
    stop: &CancellationToken,
    deadline: Instant,
) -> Result<()> {
    let length = u32::try_from(frame.bytes.len())?;
    error::within(stop, deadline, Stage::Write, async {
        output.write_all(&length.to_be_bytes()).await?;
        for chunk in frame.bytes.chunks(16 * 1024) {
            output.write_all(chunk).await?;
        }
        output.flush().await?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_wire::protocol::{CONTROL_MESSAGE_BYTES_MAX, MESSAGE_BYTES_MAX};

    fn deadline() -> Instant {
        Instant::now() + std::time::Duration::from_secs(5)
    }

    #[tokio::test]
    async fn a_frame_is_charged_exactly_its_serialized_bytes() -> Result<()> {
        let budget = Budget::default();
        let available = budget.available(StreamKind::Reports);
        let value = ("x".repeat(MESSAGE_BYTES_MAX * 3 / 4), 1234);
        let frame = encode(
            &value,
            &budget,
            StreamKind::Reports,
            &CancellationToken::new(),
            deadline(),
        )
        .await?;
        let measured = af_validate::serialized_len(&value)?;
        assert_eq!(frame.bytes.len(), measured);
        assert_eq!(frame.bytes.capacity(), measured);
        assert_eq!(available - budget.available(StreamKind::Reports), measured);
        drop(frame);
        assert_eq!(budget.available(StreamKind::Reports), available);
        Ok(())
    }

    #[tokio::test]
    async fn a_value_past_its_streams_ceiling_reserves_nothing() -> Result<()> {
        let budget = Budget::default();
        let available = budget.available(StreamKind::Control);
        let value = "x".repeat(CONTROL_MESSAGE_BYTES_MAX);
        let refused = encode(
            &value,
            &budget,
            StreamKind::Control,
            &CancellationToken::new(),
            deadline(),
        )
        .await
        .err()
        .ok_or(error::ErrorKind::Protocol)?;
        assert_eq!(refused.reason(), crate::Reason::Protocol);
        assert!(
            std::error::Error::source(&refused)
                .is_some_and(<dyn std::error::Error>::is::<garde::Report>)
        );
        assert_eq!(budget.available(StreamKind::Control), available);
        Ok(())
    }

    #[test]
    fn a_buffer_refuses_to_grow_past_its_measured_room() -> Result<()> {
        use std::io::Write as _;
        let mut buffer = Buffer::exact(&Length::prove(4, StreamKind::Work)?);
        buffer.write_all(b"1234")?;
        let refused = buffer.write(b"5").err().ok_or(error::ErrorKind::Protocol)?;
        assert_eq!(refused.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(buffer.0.capacity(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn an_encoder_flushes_without_emitting_and_hides_its_bytes() -> Result<()> {
        let budget = Budget::default();
        let mut encoder = super::super::Encoder::reserve(
            &budget,
            StreamKind::Control,
            &CancellationToken::new(),
            deadline(),
        )
        .await?;
        encoder.frame(&"private")?;
        assert_eq!(format!("{encoder:?}"), "Encoder { .. }");
        let frame = encoder
            .finish(&CancellationToken::new(), deadline())
            .await?;
        assert_eq!(frame.as_bytes(), br#""private""#);
        let reader = crate::Reader::new(
            tokio::io::empty(),
            budget,
            StreamKind::Work,
            CancellationToken::new(),
            std::time::Duration::from_secs(1),
        );
        assert_eq!(format!("{reader:?}"), "Reader { kind: Work, .. }");
        Ok(())
    }
}
