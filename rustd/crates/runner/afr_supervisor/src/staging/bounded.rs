//! A spool that takes no more than a bound: what an export's stream is
//! written through, so a guest decides nothing about how much host disk its
//! export takes.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::fs::File;
use tokio::io::AsyncWrite;

/// A spool with room for a fixed number of bytes more.
#[derive(Debug)]
pub(super) struct Bounded {
    spool: File,
    room: u64,
    outgrown: bool,
}

impl Bounded {
    /// `spool`, taking at most `room` bytes.
    pub(super) const fn new(spool: File, room: u64) -> Self {
        Self {
            spool,
            room,
            outgrown: false,
        }
    }

    /// Whether a write was refused for want of room.
    pub(super) const fn outgrown(&self) -> bool {
        self.outgrown
    }

    /// The spool, holding what was written.
    pub(super) fn into_spool(self) -> File {
        self.spool
    }
}

impl AsyncWrite for Bounded {
    /// Refuses, whole, a write the room left does not hold: nothing of it
    /// reaches the spool.
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if crate::byte_count(buf.len()) > self.room {
            self.outgrown = true;
            return Poll::Ready(Err(io::ErrorKind::FileTooLarge.into()));
        }
        let written = ready!(Pin::new(&mut self.spool).poll_write(cx, buf))?;
        self.room = self.room.saturating_sub(crate::byte_count(written));
        Poll::Ready(Ok(written))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.spool).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.spool).poll_shutdown(cx)
    }
}
