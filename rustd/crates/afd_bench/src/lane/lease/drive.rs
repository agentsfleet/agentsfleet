//! The polling loop under measurement, run once per simulated runner.
//!
//! # What one iteration is
//!
//! Exactly one `Leases::select` — the real assignment pass, timed end to end.
//! No retry, no backoff, no sleep: a runner in production re-polls on the
//! backoff its reply carries, and modelling that here would measure the model.
//! What this measures is how fast the pass answers when asked continuously.
//!
//! # A miss is data, not a failure
//!
//! `Ok(None)` means nothing was leasable this pass. Under contention that is
//! the interesting case: it is a poll that cost a readiness peek, possibly a
//! candidate query, and produced no work — which is what a runner fleet larger
//! than its ready depth spends most of its time doing.

use core::time::Duration;
use std::time::Instant;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::Leases;

use crate::abort::Abort;
use crate::error::Result;

/// What one runner's loop did in its window.
#[derive(Debug, Clone, Default)]
pub struct Polled {
    /// Polls that issued a lease.
    pub leases: u64,
    /// Polls that found nothing leasable.
    pub misses: u64,
    /// Polls the pass refused: a datastore that would not answer.
    pub failures: u64,
    /// How long each poll took, in order.
    pub durations: Vec<Duration>,
}

impl Polled {
    /// Fold another runner's loop into this one.
    pub fn absorb(&mut self, other: Self) {
        self.leases += other.leases;
        self.misses += other.misses;
        self.failures += other.failures;
        self.durations.extend(other.durations);
    }

    /// Every poll, whether or not it issued a lease.
    #[must_use]
    pub const fn polls(&self) -> u64 {
        self.leases + self.misses
    }

    /// Polls that cost something and produced no work.
    ///
    /// Measured from OUTSIDE the pass, because the pass does not publish a
    /// per-poll reason: a miss here is any poll that found nothing leasable,
    /// which under contention is dominated by fleets another runner claimed
    /// first. It is an upper bound on wasted claims, not a count of them.
    #[must_use]
    pub fn wasted_fraction(&self) -> f64 {
        let polls = self.polls();
        if polls == 0 {
            return 0.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a poll count past f64's exact range is not a run that finished"
        )]
        {
            self.misses as f64 / polls as f64
        }
    }
}

/// Poll until the deadline, `stop_after` leases, or the abort monitor fires.
///
/// A pass that FAULTS is counted, not propagated: the monitor decides when a
/// target refusing often enough is a reason to stop, and one refusal is data
/// about the window rather than the end of it.
///
/// # Errors
///
/// None today; the signature keeps the seam a future refusal can use.
pub async fn poll_until(
    leases: &Leases,
    runner: &Uuid7,
    deadline: Instant,
    stop_after: Option<u64>,
    abort: &Abort,
) -> Result<Polled> {
    let mut polled = Polled::default();
    let token = abort.token();
    while Instant::now() < deadline && !token.is_cancelled() {
        if stop_after.is_some_and(|ceiling| polled.leases >= ceiling) {
            break;
        }
        let started = Instant::now();
        match leases.select(runner, now()).await {
            Ok(acquired) => {
                polled.durations.push(started.elapsed());
                abort.record(true);
                if acquired.is_some() {
                    polled.leases += 1;
                } else {
                    polled.misses += 1;
                }
            }
            Err(_refused) => {
                polled.failures += 1;
                abort.record(false);
            }
        }
    }
    Ok(polled)
}

/// The wall clock, in the shape the assignment pass is given.
fn now() -> UnixMillis {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    UnixMillis::from_millis(i64::try_from(millis).unwrap_or(i64::MAX))
}
