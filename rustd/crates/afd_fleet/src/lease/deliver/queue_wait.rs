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

use afd_core::clock::UnixMillis;

use crate::lease::envelope::{Acquired, Kind};

/// The instant the lease was written: the poll's wall-clock `now` plus the
/// time the poll has taken since `began`.
pub(super) fn issued_at(now: UnixMillis, began: Instant) -> UnixMillis {
    let polled = i64::try_from(began.elapsed().as_millis()).unwrap_or(i64::MAX);
    now.saturating_add_millis(polled)
}

/// How long `acquired` waited for a runner, if it was read new.
pub(super) fn of(acquired: &Acquired, issued_at: UnixMillis) -> Option<i64> {
    (acquired.kind == Kind::Fresh && !acquired.redelivered).then(|| {
        issued_at
            .saturating_millis_since(acquired.event_created_at)
            .max(0)
    })
}

#[cfg(test)]
#[path = "queue_wait_tests.rs"]
mod tests;
