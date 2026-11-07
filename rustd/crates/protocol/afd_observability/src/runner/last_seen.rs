//! When a runner was last heard from, held lock-free.
//!
//! An atomic holds an integer, so the instant is stored as whole milliseconds
//! since the epoch; this type is the one place that spelling meets
//! [`Timestamp`]. Zero means never heard from: no runner reports from 1970.

use std::sync::atomic::{AtomicI64, Ordering};

use jiff::Timestamp;

/// The stored count of a runner nothing has heard from.
const NEVER_HEARD_FROM: i64 = 0;

/// One runner's last-seen instant.
#[derive(Debug, Default)]
pub(super) struct LastSeen(AtomicI64);

impl LastSeen {
    /// Records `at` as the latest word from the runner.
    pub(super) fn record(&self, at: Timestamp) {
        self.0.store(at.as_millisecond(), Ordering::Relaxed);
    }

    /// The latest word from the runner, or `None` for one never heard from.
    ///
    /// An instant at or before the epoch reads as never heard from too: a
    /// dashboard would draw it as the oldest host in the fleet.
    pub(super) fn get(&self) -> Option<Timestamp> {
        Some(self.0.load(Ordering::Relaxed))
            .filter(|count| *count > NEVER_HEARD_FROM)
            .and_then(|count| Timestamp::from_millisecond(count).ok())
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use super::LastSeen;

    #[test]
    fn a_recorded_instant_reads_back_in_whole_milliseconds() {
        let seen = LastSeen::default();

        seen.record(Timestamp::constant(1_760_000_000, 123_456_789));

        assert_eq!(
            seen.get(),
            Some(Timestamp::constant(1_760_000_000, 123_000_000))
        );
    }

    #[test]
    fn a_runner_never_heard_from_or_heard_from_before_the_epoch_reads_none() {
        let seen = LastSeen::default();
        assert_eq!(seen.get(), None, "nothing recorded");

        seen.record(Timestamp::UNIX_EPOCH);
        assert_eq!(seen.get(), None, "the epoch is the absence sentinel");

        seen.record(Timestamp::constant(-1, 0));
        assert_eq!(seen.get(), None, "no runner reports from before 1970");
    }
}
