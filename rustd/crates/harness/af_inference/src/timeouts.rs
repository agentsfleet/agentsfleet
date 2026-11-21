//! Physical provider deadlines shared by daemon and runner configuration.

use garde::Validate;
use serde::Deserialize;
use std::time::Duration;

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
            connect_timeout_seconds: super::stream::CONNECT_TIMEOUT.as_secs(),
            stream_idle_timeout_seconds: super::stream::READ_TIMEOUT.as_secs(),
        }
    }
}

/// Checked provider timeouts, passed by value without a shared mutable setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    connect: Duration,
    stream_idle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Timeouts {
    /// Existing defaults also support constant client construction.
    pub const DEFAULT: Self = Self {
        connect: super::stream::CONNECT_TIMEOUT,
        stream_idle: super::stream::READ_TIMEOUT,
    };

    /// Accepts only settings whose positive, finite bounds were checked.
    #[must_use]
    pub fn new(settings: &garde::Valid<TimeoutSettings>) -> Self {
        Self {
            connect: Duration::from_secs(settings.connect_timeout_seconds),
            stream_idle: Duration::from_secs(settings.stream_idle_timeout_seconds),
        }
    }

    /// Physical connection establishment limit.
    #[must_use]
    pub const fn connect(self) -> Duration {
        self.connect
    }

    /// Provider stream and socket inactivity limit.
    #[must_use]
    pub const fn stream_idle(self) -> Duration {
        self.stream_idle
    }
}
