//! How long a fresh event waited for a runner, as `runner_lease_issued` logs
//! it.
//!
//! The admission's stamp to the lease's write, both on the daemon replicas'
//! clocks rather than the runner's, which is another machine's. The poll's own
//! reads and writes count, measured from its start on the monotonic clock.
//!
//! Only an entry read new logs one. A reclaimed lease's stamp predates the run
//! it replaces, and an owed entry, a parked event a person just answered or a
//! re-poll after a fault, waited on something other than a runner. Two
//! replicas' clocks can disagree by a few milliseconds, so a wait that reads
//! below zero is skew and logs as zero.

use std::time::Instant;

use jiff::Timestamp;

use crate::lease::envelope::{Acquired, Kind};

/// The instant the lease was written: the poll's wall-clock `now` plus the
/// time the poll has taken since `began`.
pub(super) fn issued_at(now: Timestamp, began: Instant) -> Timestamp {
    afd_core::clock::after(now, began.elapsed())
}

/// How many milliseconds `acquired` waited for a runner, if it was read new:
/// the log field spells the span as a count.
pub(super) fn of(acquired: &Acquired, issued_at: Timestamp) -> Option<u64> {
    (acquired.kind == Kind::Fresh && !acquired.redelivered).then(|| {
        afd_core::clock::saturating_millis(afd_core::clock::elapsed(
            acquired.event_created_at,
            issued_at,
        ))
    })
}

#[cfg(test)]
#[path = "queue_wait_tests.rs"]
mod tests;
