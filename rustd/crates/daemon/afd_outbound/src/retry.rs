//! How many times an answer is offered to a vendor, and how long between.
//!
//! # The whole schedule is `backon`'s, not this crate's
//!
//! A vendor POST retry has no sibling curve to agree with, so a hand-written
//! schedule would buy nothing and cost a jitter source, an `Iterator` impl and
//! its own tests.
//!
//! [`ExponentialBuilder`] already carries every part of it: the factor, the
//! floor, the ceiling, the attempt count, and `with_jitter`. Writing those
//! five numbers is the whole module.
//!
//! # Why the delays are jittered
//!
//! Under a flat `200ms << attempt`, every worker that saw the same vendor
//! outage retries in the same millisecond, and the recovering vendor is hit by
//! the whole deployment at once — the retry storm that keeps it down. `backon`
//! adds a random offset inside each delay, which is what the retry tests'
//! "jittered" means.
//!
//! # The budget is small on purpose
//!
//! Three attempts, two sleeps, under a second of waiting. A longer budget does
//! not buy reliability here, because the claim already provides it: a row this
//! worker stops renewing lapses, and the next claim pass retakes it. What
//! the inline retry covers is the narrow case that budget suits — a 429, a
//! brief 5xx — and holding the worker on one job for a minute would stall every
//! answer behind it, because delivery is serial.

use std::time::Duration;

use backon::ExponentialBuilder;
use garde::Validate;
use serde::Deserialize;

/// Inline delivery attempts; a durable delivery remains claimable after exhaustion.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct RetrySettings {
    /// Total sends, including the first send.
    #[garde(range(min = 1, max = 10))]
    pub max_attempts: usize,
    /// Initial jittered exponential pause.
    #[garde(range(min = 1, max = self.max_backoff_ms))]
    pub initial_backoff_ms: u64,
    /// Exponential ceiling before jitter; cancellation and claim loss still win.
    #[garde(range(min = 1, max = 30_000))]
    pub max_backoff_ms: u64,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            max_attempts: DELIVERY_ATTEMPTS,
            initial_backoff_ms: 200,
            max_backoff_ms: 800,
        }
    }
}

/// Checked inline retry policy, independent of the durable claim scan cadence.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    attempts: usize,
    first: Duration,
    maximum: Duration,
}

impl RetryPolicy {
    /// Existing delivery behavior.
    pub const DEFAULT: Self = Self {
        attempts: DELIVERY_ATTEMPTS,
        first: FIRST_DELAY,
        maximum: MAX_DELAY,
    };

    /// Accepts only finite positive startup settings.
    #[must_use]
    pub fn new(settings: &garde::Valid<RetrySettings>) -> Self {
        Self {
            attempts: settings.max_attempts,
            first: Duration::from_millis(settings.initial_backoff_ms),
            maximum: Duration::from_millis(settings.max_backoff_ms),
        }
    }

    /// The standard library schedule with total attempts translated to retries.
    #[must_use]
    pub const fn schedule(self) -> ExponentialBuilder {
        ExponentialBuilder::new()
            .with_min_delay(self.first)
            .with_max_delay(self.maximum)
            .with_max_times(self.attempts - 1)
            .with_jitter()
    }
}

/// How many times one answer is offered before it is given up on.
///
/// Three, because a crash is already covered by the lapsed claim, so this
/// budget only has to cover a transient.
pub const DELIVERY_ATTEMPTS: usize = 3;

/// The first delay, and the base the rest double from.
const FIRST_DELAY: Duration = Duration::from_millis(200);

/// The ceiling one delay may reach.
///
/// Two sleeps come to 600ms before jitter and under a second with it. Delivery
/// is serial, so this wait is paid by every answer queued behind this one.
const MAX_DELAY: Duration = Duration::from_millis(800);
