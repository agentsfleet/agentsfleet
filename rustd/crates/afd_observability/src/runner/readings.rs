//! What a collection callback reads off the table: one reading per admitted
//! runner, for the two gauges the census declares.
//!
//! Split from [`super`] for length. The table and its admission stay there;
//! this is the read side, and it only ever reads atomics.

use std::sync::atomic::Ordering;

use super::{Counters, NEVER_HEARD_FROM, RunnerMetrics};

impl RunnerMetrics {
    /// What every admitted runner was last heard from at, in seconds.
    ///
    /// Milliseconds are the storage and seconds are the wire, because the
    /// census declares this family in seconds — converting at the callback is
    /// what keeps one conversion in the process rather than one per reader.
    ///
    /// A runner that has never been heard from publishes NO reading: a zero
    /// there would read as 1970, which every dashboard would draw as the
    /// oldest host in the fleet.
    #[must_use]
    pub fn last_seen_readings(&self) -> Vec<crate::metrics::instrument::Reading> {
        self.readings(|counters| {
            let stamp = counters.last_seen_ms.load(Ordering::Relaxed);
            // Zero is the ABSENCE sentinel, not a time. A runner admitted and
            // never heard from would otherwise publish the epoch, which every
            // dashboard draws as the oldest host in the fleet — and no runner
            // reports from 1970, so nothing real is lost by refusing it. The
            // delivery span refuses a pre-epoch start for the same reason.
            (stamp > NEVER_HEARD_FROM)
                .then(|| jiff::Timestamp::from_millisecond(stamp).ok())
                .flatten()
                .and_then(|at| u64::try_from(at.as_second()).ok())
        })
    }

    /// How many leases each admitted runner is believed to hold.
    ///
    /// Best effort, and knowingly: a lease abandoned by a dead runner expires
    /// by the clock with no report to decrement against, so a crashed host's
    /// reading stays high until the process restarts. The liveness sweeper's
    /// row is the authority; this is a hint beside it.
    #[must_use]
    pub fn active_lease_readings(&self) -> Vec<crate::metrics::instrument::Reading> {
        self.readings(|counters| u64::try_from(counters.active_leases.load(Ordering::Relaxed)).ok())
    }

    /// One reading per admitted runner, for those that have one.
    ///
    /// The walk locks one shard at a time rather than the whole table, so a
    /// runner admitted while it runs may land on either side of it. That is
    /// the right trade for a gauge: a collection samples a moving deployment
    /// either way, the next one is seconds later, and the alternative is
    /// holding every publisher still so that one snapshot agrees with itself.
    /// The closure reads atomics and never touches the table, which is what
    /// keeps the shard this holds from deadlocking against itself.
    fn readings<F>(&self, read: F) -> Vec<crate::metrics::instrument::Reading>
    where
        F: Fn(&Counters) -> Option<u64>,
    {
        self.series
            .iter()
            .filter_map(|series| {
                read(series.value()).map(|value| crate::metrics::instrument::Reading {
                    attributes: vec![opentelemetry::KeyValue::new(
                        crate::semconv::LABEL_RUNNER_ID,
                        series.key().to_string(),
                    )],
                    value,
                })
            })
            .collect()
    }
}
