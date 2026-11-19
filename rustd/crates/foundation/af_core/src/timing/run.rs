//! Validated deployment run timing, shared by every daemon store.

use std::time::Duration;

use garde::Validate;
use serde::Deserialize;

/// Integer-second run lifetime strictly above the complete renewal budget.
pub const MIN_RUN_SECONDS: u64 = super::RENEWAL_TICK
    .saturating_add(super::STREAM_ANSWER_TIMEOUT)
    .saturating_add(super::STREAM_RECOVERY_BUDGET)
    .saturating_add(super::RENEWAL_WINDOW)
    .as_secs()
    + 1;

/// Largest whole-second lifetime below the existing runtime ceiling.
pub const MAX_RUN_SECONDS: u64 = super::MAX_RUNTIME
    .saturating_sub(Duration::from_nanos(1))
    .as_secs();

/// Cross-checked run, heartbeat and sandbox timing resolved before startup.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct RunSettings {
    /// Every renewal leaves the complete connection-recovery safety budget.
    #[garde(range(min = MIN_RUN_SECONDS, max = MAX_RUN_SECONDS))]
    pub ttl_seconds: u64,
    /// Saved when a run is issued; later deployment changes cannot extend it.
    #[garde(range(min = self.ttl_seconds.saturating_add(1), max = super::MAX_RUNTIME.as_secs()))]
    pub max_runtime_seconds: u64,
    /// The daemon advertises this cadence and uses it for connection silence.
    #[garde(range(min = 1, max = self.offline_seconds().saturating_sub(1) / 2))]
    pub heartbeat_interval_seconds: u64,
    /// Omission retains three run lifetimes; an explicit threshold allows a full run.
    #[garde(range(min = self.ttl_seconds, max = super::MAX_RUNTIME.as_secs().saturating_mul(3)))]
    pub offline_after_seconds: Option<u64>,
    /// Continuous healthy heartbeats required before automatic quarantine lifts.
    #[garde(range(min = self.heartbeat_interval_seconds.saturating_add(1), max = self.offline_seconds().saturating_sub(1)))]
    pub healthy_before_uncordon_seconds: u64,
    /// Maximum idle reservation for a frozen sandbox after a completed run.
    #[garde(range(min = 1, max = super::MAX_RUNTIME.as_secs()))]
    pub sandbox_idle_hold_seconds: u64,
}

impl RunSettings {
    fn offline_seconds(self) -> u64 {
        self.offline_after_seconds
            .unwrap_or_else(|| self.ttl_seconds.saturating_mul(3))
    }
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            ttl_seconds: super::RUN_TTL.as_secs(),
            max_runtime_seconds: super::MAX_RUNTIME.as_secs(),
            heartbeat_interval_seconds: super::HEARTBEAT_INTERVAL.as_secs(),
            offline_after_seconds: None,
            healthy_before_uncordon_seconds: super::UNCORDON_AFTER.as_secs(),
            sandbox_idle_hold_seconds: super::SANDBOX_HOLD_IDLE.as_secs(),
        }
    }
}

/// A run clock whose bounds have already been checked at startup.
#[derive(Debug, Clone, Copy)]
pub struct RunClock {
    ttl: u64,
    max_runtime: u64,
    heartbeat: u64,
    offline_after: u64,
    uncordon_after: u64,
    sandbox_idle_hold: u64,
}

impl Default for RunClock {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl RunClock {
    /// Default timing also supports stores constructed in constant contexts.
    pub const DEFAULT: Self = Self {
        ttl: super::RUN_TTL.as_secs(),
        max_runtime: super::MAX_RUNTIME.as_secs(),
        heartbeat: super::HEARTBEAT_INTERVAL.as_secs(),
        offline_after: super::RUNNER_OFFLINE_AFTER.as_secs(),
        uncordon_after: super::UNCORDON_AFTER.as_secs(),
        sandbox_idle_hold: super::SANDBOX_HOLD_IDLE.as_secs(),
    };

    /// Keeps validated values without exposing an unchecked constructor.
    #[must_use]
    pub fn new(settings: &garde::Valid<RunSettings>) -> Self {
        Self {
            ttl: settings.ttl_seconds,
            max_runtime: settings.max_runtime_seconds,
            heartbeat: settings.heartbeat_interval_seconds,
            offline_after: settings.offline_seconds(),
            uncordon_after: settings.healthy_before_uncordon_seconds,
            sandbox_idle_hold: settings.sandbox_idle_hold_seconds,
        }
    }

    /// Span granted by a new or successfully renewed run.
    #[must_use]
    pub const fn ttl(self) -> Duration {
        Duration::from_secs(self.ttl)
    }

    /// Total runtime saved with each newly issued run.
    #[must_use]
    pub const fn max_runtime(self) -> Duration {
        Duration::from_secs(self.max_runtime)
    }

    /// Cadence advertised to the runner on welcome and heartbeat replies.
    #[must_use]
    pub const fn heartbeat_interval(self) -> Duration {
        Duration::from_secs(self.heartbeat)
    }

    /// Silence after which the runner is considered offline.
    #[must_use]
    pub const fn offline_after(self) -> Duration {
        Duration::from_secs(self.offline_after)
    }

    /// Uninterrupted healthy return required to lift automatic quarantine.
    #[must_use]
    pub const fn uncordon_after(self) -> Duration {
        Duration::from_secs(self.uncordon_after)
    }

    /// Maximum reservation for a frozen sandbox after a completed run.
    #[must_use]
    pub const fn sandbox_idle_hold(self) -> Duration {
        Duration::from_secs(self.sandbox_idle_hold)
    }
}
