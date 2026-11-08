//! Reserve the proven frame length before reading directly into zeroed memory.

use std::time::Duration;

use af_wire::protocol::Lane;
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::{Budget, OwnedFrame};
use crate::{
    Result, Stage,
    error::{self, ErrorKind},
    limits::Length,
};

/// A frame reader; partial headers and payloads share one absolute deadline.
pub struct Reader<R> {
    input: R,
    budget: Budget,
    lane: Lane,
    stop: CancellationToken,
    partial_timeout: Duration,
}

impl<R> std::fmt::Debug for Reader<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("lane", &self.lane)
            .finish_non_exhaustive()
    }
}

impl<R: AsyncRead + Unpin> Reader<R> {
    /// Bind one lane to its byte pool and parent cancellation.
    pub const fn new(
        input: R,
        budget: Budget,
        lane: Lane,
        stop: CancellationToken,
        partial_timeout: Duration,
    ) -> Self {
        Self {
            input,
            budget,
            lane,
            stop,
            partial_timeout,
        }
    }

    /// Read one payload, refusing an oversized prefix before reserving memory.
    ///
    /// # Errors
    /// Fails on cancellation, a partial-frame deadline, invalid length or I/O.
    pub async fn next(&mut self) -> Result<Option<OwnedFrame>> {
        let mut prefix = [0; 4];
        let [first, ..] = &mut prefix;
        let received = tokio::select! {
            biased;
            () = self.stop.cancelled() => return Err(ErrorKind::Cancelled.into()),
            read = self.input.read(std::slice::from_mut(first)) => read?,
        };
        if received == 0 {
            return Ok(None);
        }
        let deadline = Instant::now() + self.partial_timeout;
        let stop = self.stop.clone();
        error::within(&stop, deadline, Stage::Read, self.frame(prefix, deadline))
            .await
            .map(Some)
    }

    async fn frame(&mut self, mut prefix: [u8; 4], deadline: Instant) -> Result<OwnedFrame> {
        let [_, tail @ ..] = &mut prefix;
        self.input.read_exact(tail).await?;
        let length = Length::prove(usize::try_from(u32::from_be_bytes(prefix))?)?;
        let reservation = self
            .budget
            .reserve(self.lane, length.bytes, &self.stop, deadline)
            .await?;
        let mut frame = OwnedFrame::allocate(&length, reservation);
        self.input.read_exact(&mut frame.bytes).await?;
        Ok(frame)
    }
}
