//! The deployment ceiling's sampled figure, and when it is resampled.
//!
//! # Why the figure is sampled and not counted
//!
//! The deployment budget asks how many admitted rows await a receipt. Counted
//! per admission, that question costs a walk of every waiting row — the
//! partial index on `receipt IS NULL` keeps the walk off the table but not off
//! the rows, so the cost rises with the backlog and peaks when the deployment
//! is already behind. A valve that gets heavier the harder it is pressed is
//! the wrong shape for the hot path every producer takes.
//!
//! So the figure is read occasionally and held here, and the hot path adds
//! what this process has admitted since. Two atomic loads and an add.
//!
//! # The estimate is deliberately high, never low
//!
//! [`Ceiling::estimate`] is the last sample plus every admission this process
//! committed after it. A row leaves the counted state only when a receipt is
//! recorded, so the sum can exceed what the table holds and cannot fall short
//! of it on this process's own traffic. The refusal therefore arrives early
//! rather than late, which is the direction a safety valve should err in.
//!
//! Two gaps remain, both bounded and both closed by the next sample: receipts
//! recorded since the sample are not yet subtracted, so a drained backlog
//! refuses for up to [`REFUSING_INTERVAL`]; and a sibling replica's
//! admissions are in no local counter, so the estimate trails that replica's
//! rate for up to [`SAMPLE_INTERVAL`]. The ledger's own `receipt IS NULL`
//! count stays the authority — `docs/architecture/datastore_scaling.md`
//! records the figure this replaces and the bound above.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::time::Duration;

use afd_core::clock::UnixMillis;

use crate::budget::REPLAY_BACKLOG_BUDGET;

/// How many admissions pass between samples.
///
/// The sample's cost is one walk of the waiting rows, so this is the divisor
/// that turns it into a per-admission cost: at the ceiling, a walk of
/// [`REPLAY_BACKLOG_BUDGET`] rows spread over this many admissions.
const SAMPLE_EVERY: u64 = 1_000;

/// How stale the figure may be while the estimate is within budget.
///
/// A local counter already covers this process's own admissions, so the only
/// thing this bounds is how long the figure trails a sibling replica's.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);

/// How stale the figure may be while the estimate is refusing.
///
/// Shorter than [`SAMPLE_INTERVAL`] because a refusal must clear as the
/// sweeper drains the backlog: the local counter only ever raises the
/// estimate, so nothing but a fresh sample can end the refusal.
const REFUSING_INTERVAL: Duration = Duration::from_millis(500);

/// The instant no sample has been taken at, which is every instant before the
/// first one.
const NEVER: i64 = i64::MIN;

// Decidable where they are written, so checked there. A runtime test could
// only re-assert a constant a reader can see, and would report a bad edit as a
// red suite instead of a build that does not produce a binary.
const _: () = {
    assert!(
        SAMPLE_EVERY > 0,
        "a zero divisor samples on every admission, which is the cost this exists to remove"
    );
    assert!(
        SAMPLE_EVERY <= REPLAY_BACKLOG_BUDGET,
        "sampling less often than the ceiling is deep admits a whole budget between samples"
    );
    // The refusal cannot clear without a sample, so the refusing figure is the
    // fresher of the two. Equal intervals would leave a drained backlog
    // refusing for as long as a healthy one waits to notice a sibling.
    assert!(
        REFUSING_INTERVAL.as_millis() < SAMPLE_INTERVAL.as_millis(),
        "a refusal clears no faster than its figure is resampled"
    );
};

/// The sampled `receipt IS NULL` figure, and the admissions counted onto it.
///
/// One per ledger, shared by every clone of it, so the figure a sample
/// publishes is the figure the next admission reads. Held behind
/// [`crate::Admissions`]; `budget.rs` owns the refresh that writes it.
#[derive(Debug)]
pub(crate) struct Ceiling {
    /// Rows awaiting a receipt when the figure was last read.
    sampled: AtomicU64,
    /// Admissions this process has committed since that read.
    since: AtomicU64,
    /// When that read happened, or [`NEVER`].
    read_at: AtomicI64,
    /// Held by whichever task is taking the next sample.
    sampling: AtomicBool,
}

impl Default for Ceiling {
    fn default() -> Self {
        Self {
            sampled: AtomicU64::new(0),
            since: AtomicU64::new(0),
            read_at: AtomicI64::new(NEVER),
            sampling: AtomicBool::new(false),
        }
    }
}

impl Ceiling {
    /// What this process believes awaits a receipt deployment-wide.
    ///
    /// Saturating, so a counter that has outrun the figure it rides on cannot
    /// wrap into a small number and admit a deployment that should refuse.
    pub(crate) fn estimate(&self) -> u64 {
        self.sampled
            .load(Ordering::Relaxed)
            .saturating_add(self.since.load(Ordering::Relaxed))
    }

    /// Counts one committed admission onto the figure.
    pub(crate) fn admitted(&self) {
        self.since.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether the figure is old enough to read again.
    ///
    /// `estimate` and `budget` decide WHICH interval applies, so a refusing
    /// deployment resamples on the short one and clears promptly.
    pub(crate) fn due(&self, now: UnixMillis, estimate: u64, budget: u64) -> bool {
        let read_at = self.read_at.load(Ordering::Relaxed);
        if read_at == NEVER {
            return true;
        }
        if self.since.load(Ordering::Relaxed) >= SAMPLE_EVERY {
            return true;
        }
        let interval = if estimate >= budget {
            REFUSING_INTERVAL
        } else {
            SAMPLE_INTERVAL
        };
        let age = now.as_millis().saturating_sub(read_at);
        u128::try_from(age).unwrap_or(0) >= interval.as_millis()
    }

    /// Takes the right to sample, or answers that another task holds it.
    ///
    /// The loser does NOT re-read the figure (RULE CAS): the winner may be
    /// mid-publish, and a torn pair reads worse than the value the loser
    /// already has. It proceeds on its own estimate and the next admission
    /// picks up the fresh one.
    pub(crate) fn claim(&self) -> Claim<'_> {
        let won = self
            .sampling
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok();
        // Read BEFORE the caller issues its query, so `publish` takes off only
        // what the figure it publishes already accounts for.
        let counted = self.since.load(Ordering::Relaxed);
        Claim {
            ceiling: self,
            won,
            counted,
        }
    }
}

/// The right to take the next sample, released however the sample ends.
///
/// A sample that fails releases without publishing, which leaves the previous
/// figure standing: the module note in `budget.rs` is explicit that a budget
/// which cannot be read is not exceeded, and a failed read here is that same
/// outage seen from the ledger's side.
pub(crate) struct Claim<'a> {
    ceiling: &'a Ceiling,
    won: bool,
    counted: u64,
}

impl Claim<'_> {
    /// Whether this claim is the one that should sample.
    pub(crate) const fn won(&self) -> bool {
        self.won
    }

    /// Publishes a figure read while this claim was held.
    ///
    /// Only the count captured when the claim was taken comes off: admissions
    /// committed while the read was in flight stay counted, because the figure
    /// may predate them. They are counted twice at worst, which overstates the
    /// backlog and refuses early — the direction this whole figure errs in.
    ///
    /// Saturating because the claim is exclusive but the subtraction is not
    /// worth a wrap on the one path that would make a full deployment read
    /// empty.
    pub(crate) fn publish(&self, rows: u64, now: UnixMillis) {
        let counted = self.counted;
        self.ceiling.sampled.store(rows, Ordering::Relaxed);
        let _ = self
            .ceiling
            .since
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |since| {
                Some(since.saturating_sub(counted))
            });
        self.ceiling
            .read_at
            .store(now.as_millis(), Ordering::Relaxed);
    }
}

impl Drop for Claim<'_> {
    /// Releases the right on every path out, including the failed read and the
    /// early return, so one failed sample cannot park the figure forever.
    fn drop(&mut self) {
        if self.won {
            self.ceiling.sampling.store(false, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests;
