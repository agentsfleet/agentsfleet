//! Bound serialization and every in-progress frame write.

use af_wire::protocol::{Lane, MESSAGE_BYTES_MAX};
use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::{Budget, OwnedFrame};
use crate::{Result, Stage, error, limits::Length};

struct Buffer(Zeroizing<Vec<u8>>);

// Reallocation can temporarily retain both the old and new allocations.
const ENCODING_BYTES: usize = MESSAGE_BYTES_MAX * 2;

impl Buffer {
    fn finish(self, mut reservation: tokio::sync::OwnedSemaphorePermit) -> Result<OwnedFrame> {
        let allocation = Length::prove(self.0.capacity())?;
        let _unused = reservation.split(ENCODING_BYTES - allocation.bytes);
        Ok(OwnedFrame {
            bytes: self.0,
            _reservation: reservation,
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
            let additional = capacity - self.0.len();
            self.0.reserve_exact(additional);
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
    lane: Lane,
    stop: &CancellationToken,
    deadline: Instant,
) -> Result<OwnedFrame> {
    let reservation = budget.reserve(lane, ENCODING_BYTES, stop, deadline).await?;
    let mut output = Buffer(Zeroizing::new(Vec::new()));
    serde_json::to_writer(&mut output, value)?;
    output.finish(reservation)
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
        let available = budget.available(Lane::Reports);
        let value = ("x".repeat(MESSAGE_BYTES_MAX * 3 / 4), 1234);
        let frame = encode(
            &value,
            &budget,
            Lane::Reports,
            &CancellationToken::new(),
            Instant::now() + std::time::Duration::from_secs(5),
        )
        .await?;
        assert!(frame.bytes.capacity() <= MESSAGE_BYTES_MAX);
        assert!(frame.bytes.capacity() >= frame.bytes.len());
        assert_eq!(
            available - budget.available(Lane::Reports),
            frame.bytes.capacity()
        );
        drop(frame);
        assert_eq!(budget.available(Lane::Reports), available);
        Ok(())
    }
}
