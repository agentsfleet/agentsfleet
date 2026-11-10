//! Bound serialization and every in-progress frame write.

use af_wire::protocol::{MESSAGE_BYTES_MAX, StreamKind};
use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::{Budget, Encoder, OwnedFrame};
use crate::{Result, Stage, error, limits::Length};

#[derive(Default)]
pub(super) struct Buffer(Zeroizing<Vec<u8>>);

// Reallocation can temporarily retain both the old and new allocations.
pub(super) const ENCODING_BYTES: usize = MESSAGE_BYTES_MAX * 2;

impl Buffer {
    pub(super) fn finish(
        self,
        mut reservation: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<OwnedFrame> {
        let allocation = Length::prove(self.0.capacity())?;
        let _unused = reservation.split(ENCODING_BYTES - allocation.bytes);
        Ok(OwnedFrame {
            bytes: self.0,
            reservation,
        })
    }
}

impl std::io::Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Outgoing trusted values still receive a hard allocation ceiling.
        if buf.len() > MESSAGE_BYTES_MAX - self.0.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame exceeds maximum length",
            ));
        }
        let needed = self.0.len() + buf.len();
        if needed > self.0.capacity() {
            let capacity = needed.max(self.0.capacity() * 2).min(MESSAGE_BYTES_MAX);
            // Vec's automatic doubling could otherwise exceed the frame ceiling.
            let mut grown = Zeroizing::new(Vec::with_capacity(capacity));
            grown.extend_from_slice(&self.0);
            // Replacing the owner wipes the previous plaintext allocation before freeing it.
            self.0 = grown;
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Reserve serialization's peak memory, then retain its actual allocation charge.
///
/// # Errors
/// Refuses unavailable memory, oversized values and failed serialization.
pub async fn encode<T: Serialize + Sync>(
    value: &T,
    budget: &Budget,
    kind: StreamKind,
    stop: &CancellationToken,
    deadline: Instant,
) -> Result<OwnedFrame> {
    let mut output = Encoder::reserve(budget, kind, stop, deadline).await?;
    serde_json::to_writer(&mut output, value)?;
    output.finish()
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

    #[tokio::test]
    async fn serialization_growth_stays_bounded_and_retained_capacity_remains_charged() -> Result<()>
    {
        let budget = Budget::default();
        let available = budget.available(StreamKind::Reports);
        let value = ("x".repeat(MESSAGE_BYTES_MAX * 3 / 4), 1234);
        let frame = encode(
            &value,
            &budget,
            StreamKind::Reports,
            &CancellationToken::new(),
            Instant::now() + std::time::Duration::from_secs(5),
        )
        .await?;
        assert!(frame.bytes.capacity() <= MESSAGE_BYTES_MAX);
        assert!(frame.bytes.capacity() >= frame.bytes.len());
        assert_eq!(
            available - budget.available(StreamKind::Reports),
            frame.bytes.capacity()
        );
        drop(frame);
        assert_eq!(budget.available(StreamKind::Reports), available);
        Ok(())
    }

    #[tokio::test]
    async fn an_encoder_flushes_without_emitting_and_hides_its_bytes() -> Result<()> {
        use std::io::Write as _;
        let budget = Budget::default();
        let mut encoder = Encoder::reserve(
            &budget,
            StreamKind::Control,
            &CancellationToken::new(),
            Instant::now() + std::time::Duration::from_secs(5),
        )
        .await?;
        encoder.write_all(br#""private""#)?;
        encoder.flush()?;
        assert_eq!(format!("{encoder:?}"), "Encoder { .. }");
        assert_eq!(encoder.finish()?.as_bytes(), br#""private""#);
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
