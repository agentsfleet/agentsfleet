//! The deadlines a turn is read under. A caller's own cancellation and an
//! earlier deadline of its own still win.
//!
//! There is no connect deadline here: the transport owns its connection, and
//! the default one leaves connecting to the operating system.

use std::time::Duration;

use super::count::{COUNT_REPLY_BYTES, COUNT_TIMEOUT};
use super::stream::READ_TIMEOUT;

/// Checked provider deadlines, passed by value without a shared mutable
/// setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    stream_idle: Duration,
    count: Duration,
    count_reply_bytes: usize,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Timeouts {
    /// 300 s of stream silence, and a native count of 10 s and 16 KiB.
    pub const DEFAULT: Self = Self {
        stream_idle: READ_TIMEOUT,
        count: COUNT_TIMEOUT,
        count_reply_bytes: COUNT_REPLY_BYTES,
    };

    /// The same timeouts, a provider silent for `stream_idle` ending its turn.
    #[must_use]
    pub const fn with_stream_idle(mut self, stream_idle: Duration) -> Self {
        self.stream_idle = stream_idle;
        self
    }

    /// The same timeouts, a native token count taking at most `count` and
    /// its reply at most `reply_bytes`.
    #[must_use]
    pub const fn with_count(mut self, count: Duration, reply_bytes: usize) -> Self {
        self.count = count;
        self.count_reply_bytes = reply_bytes;
        self
    }

    /// How long a native token count may take.
    #[must_use]
    pub const fn count(self) -> Duration {
        self.count
    }

    /// The largest native token-count reply accepted, in bytes.
    #[must_use]
    pub const fn count_reply_bytes(self) -> usize {
        self.count_reply_bytes
    }

    /// Provider stream and socket inactivity limit.
    #[must_use]
    pub const fn stream_idle(self) -> Duration {
        self.stream_idle
    }
}
