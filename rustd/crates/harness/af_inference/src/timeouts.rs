//! Physical provider deadlines shared by daemon and runner configuration:
//! the connect bound agentsfleet's guarded client takes, beside the
//! deadlines a turn is read under.

use garde::Validate;
use serde::Deserialize;
use std::time::Duration;

/// Shared connection establishment bound for guarded physical senders.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Startup settings; caller cancellation and earlier absolute deadlines still win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct TimeoutSettings {
    /// Maximum time to establish a physical provider connection.
    #[garde(range(min = 1, max = 120))]
    pub connect_timeout_seconds: u64,
    /// Maximum silence between parsed provider events or transport reads.
    #[garde(range(min = 1, max = 3600))]
    pub stream_idle_timeout_seconds: u64,
}

impl Default for TimeoutSettings {
    fn default() -> Self {
        Self {
            connect_timeout_seconds: CONNECT_TIMEOUT.as_secs(),
            stream_idle_timeout_seconds: guffbolt_ai::wire::Timeouts::DEFAULT
                .stream_idle()
                .as_secs(),
        }
    }
}

/// Checked provider timeouts, passed by value without a shared mutable setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    connect: Duration,
    wire: guffbolt_ai::wire::Timeouts,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Timeouts {
    /// Existing defaults also support constant client construction.
    pub const DEFAULT: Self = Self {
        connect: CONNECT_TIMEOUT,
        wire: guffbolt_ai::wire::Timeouts::DEFAULT,
    };

    /// Accepts only settings whose positive, finite bounds were checked.
    #[must_use]
    pub fn new(settings: &garde::Valid<TimeoutSettings>) -> Self {
        Self {
            connect: Duration::from_secs(settings.connect_timeout_seconds),
            wire: guffbolt_ai::wire::Timeouts::DEFAULT
                .with_stream_idle(Duration::from_secs(settings.stream_idle_timeout_seconds)),
        }
    }

    /// The same timeouts, a native token count taking at most `count` and
    /// its reply at most `reply_bytes`.
    #[must_use]
    pub const fn with_count(mut self, count: Duration, reply_bytes: usize) -> Self {
        self.wire = self.wire.with_count(count, reply_bytes);
        self
    }

    /// How long a native token count may take.
    #[must_use]
    pub const fn count(self) -> Duration {
        self.wire.count()
    }

    /// The largest native token-count reply accepted, in bytes.
    #[must_use]
    pub const fn count_reply_bytes(self) -> usize {
        self.wire.count_reply_bytes()
    }

    /// Physical connection establishment limit.
    #[must_use]
    pub const fn connect(self) -> Duration {
        self.connect
    }

    /// Provider stream and socket inactivity limit.
    #[must_use]
    pub const fn stream_idle(self) -> Duration {
        self.wire.stream_idle()
    }

    /// The deadlines a turn is read under.
    #[must_use]
    pub const fn wire(self) -> guffbolt_ai::wire::Timeouts {
        self.wire
    }
}
