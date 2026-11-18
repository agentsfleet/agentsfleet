//! Validated deployment lease timing, shared by every daemon store.

use std::time::Duration;

use garde::Validate;
use serde::Deserialize;

/// Integer-second lease lifetime strictly above the complete renewal budget.
pub const MIN_LEASE_SECONDS: u64 = super::RENEWAL_TICK
    .saturating_add(super::STREAM_ANSWER_TIMEOUT)
    .saturating_add(super::STREAM_RECOVERY_BUDGET)
    .saturating_add(super::RENEWAL_WINDOW)
    .as_secs()
    + 1;

/// Largest whole-second lifetime below the existing runtime ceiling.
pub const MAX_LEASE_SECONDS: u64 = super::MAX_RUNTIME
    .saturating_sub(Duration::from_nanos(1))
    .as_secs();

/// The existing renewal, reconnect and cleanup windows require more than 32 seconds.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct LeaseSettings {
    /// Each grant and renewal remains below the twelve-hour runtime ceiling.
    #[garde(range(min = MIN_LEASE_SECONDS, max = MAX_LEASE_SECONDS))]
    pub ttl_seconds: u64,
}

impl Default for LeaseSettings {
    fn default() -> Self {
        Self {
            ttl_seconds: super::LEASE_TTL.as_secs(),
        }
    }
}

/// A lease clock whose bounds have already been checked at startup.
#[derive(Debug, Clone, Copy)]
pub struct LeaseClock {
    ttl: Duration,
}

impl Default for LeaseClock {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl LeaseClock {
    /// Default timing also supports stores constructed in constant contexts.
    pub const DEFAULT: Self = Self {
        ttl: super::LEASE_TTL,
    };

    /// Keeps the validated value without exposing an unchecked constructor.
    #[must_use]
    pub fn new(settings: &garde::Valid<LeaseSettings>) -> Self {
        Self {
            ttl: Duration::from_secs(settings.ttl_seconds),
        }
    }

    /// Span granted by a new or successfully renewed lease.
    #[must_use]
    pub const fn ttl(self) -> Duration {
        self.ttl
    }

    /// Three missed lease lifetimes determine a silent host's offline state.
    #[must_use]
    pub const fn offline_after(self) -> Duration {
        self.ttl.saturating_mul(3)
    }
}
