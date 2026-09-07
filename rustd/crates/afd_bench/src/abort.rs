//! Stopping a run when the target starts refusing, instead of loading it
//! harder.
//!
//! # A failing target is not a slow one
//!
//! A lane exists to find a ceiling, and a target answering errors has already
//! been found past it. Continuing would only make the environment worse for
//! whoever else is on it — which on a deployed profile is the reason caps exist
//! at all. So every driver records each operation's outcome here, and the
//! moment the failure fraction crosses the profile's threshold the run is
//! cancelled, the result records the abort, and the rate reached before it is
//! reported as what it was.
//!
//! # A minimum sample, so one early error is not an abort
//!
//! The first operation failing is a 100% error rate. Judging before
//! [`MINIMUM_SAMPLE`] outcomes have landed would abort every run whose first
//! call hit a cold connection.

use std::sync::atomic::{AtomicU64, Ordering};

use tokio_util::sync::CancellationToken;

use crate::report::AbortRecord as Recorded;

/// Outcomes that must land before the fraction is judged.
pub const MINIMUM_SAMPLE: u64 = 20;

/// Failures in a row that stop a run whatever the cumulative fraction says.
///
/// A target that starts refusing LATE has a long healthy history to dilute
/// its failures: at a ten percent threshold, a thousand successes buy a
/// hundred and eleven refusals before the fraction trips, each one costing a
/// timeout against the target the monitor exists to spare. A run of refusals
/// is the same signal without the dilution.
pub const CONSECUTIVE_FAILURES: u64 = 10;

/// Watches a run's failure fraction and cancels it past the threshold.
#[derive(Debug)]
pub struct Abort {
    threshold: f64,
    attempts: AtomicU64,
    failures: AtomicU64,
    streak: AtomicU64,
    token: CancellationToken,
}

impl Abort {
    /// A monitor that cancels past `threshold`, the profile's abort rate.
    #[must_use]
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            attempts: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            streak: AtomicU64::new(0),
            token: CancellationToken::new(),
        }
    }

    /// The token a driver's loop checks each iteration.
    #[must_use]
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Record one operation's outcome, cancelling if the run should stop.
    pub fn record(&self, succeeded: bool) {
        let attempts = self.attempts.fetch_add(1, Ordering::Relaxed) + 1;
        let (failures, streak) = if succeeded {
            self.streak.store(0, Ordering::Relaxed);
            (self.failures.load(Ordering::Relaxed), 0)
        } else {
            (
                self.failures.fetch_add(1, Ordering::Relaxed) + 1,
                self.streak.fetch_add(1, Ordering::Relaxed) + 1,
            )
        };
        let past_fraction =
            attempts >= MINIMUM_SAMPLE && fraction(failures, attempts) > self.threshold;
        if past_fraction || streak >= CONSECUTIVE_FAILURES {
            self.token.cancel();
        }
    }

    /// Whether the run was stopped.
    #[must_use]
    pub fn fired(&self) -> bool {
        self.token.is_cancelled()
    }

    /// The failure fraction observed so far.
    #[must_use]
    pub fn observed(&self) -> f64 {
        fraction(
            self.failures.load(Ordering::Relaxed),
            self.attempts.load(Ordering::Relaxed),
        )
    }

    /// What the result file records, if the run was stopped.
    #[must_use]
    pub fn recorded(&self) -> Option<Recorded> {
        self.fired().then(|| Recorded {
            observed_error_rate: self.observed(),
            threshold: self.threshold,
        })
    }
}

/// `failures / attempts`, or zero before anything was attempted.
fn fraction(failures: u64, attempts: u64) -> f64 {
    if attempts == 0 {
        return 0.0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "an attempt count past f64's exact range is not a run that finished"
    )]
    {
        failures as f64 / attempts as f64
    }
}

#[cfg(test)]
mod tests;
